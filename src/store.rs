use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::spawn;
use tokio::sync::{Notify, RwLock};
use tokio::time::{Duration, Instant, sleep_until};

type Key = Vec<u8>;
type ExpirationEntry = Reverse<(Instant, Key)>;
type ExpirationHeap = BinaryHeap<ExpirationEntry>;

#[derive(Clone)]
pub struct Store {
    hashmap: Arc<RwLock<HashMap<Vec<u8>, StoreValue>>>,
    expiration_heap: Arc<RwLock<ExpirationHeap>>,
    wakeup: Arc<Notify>,
}

impl Store {
    pub fn new() -> Store {
        Self::from_parts(HashMap::new(), ExpirationHeap::new())
    }

    fn from_parts(hashmap: HashMap<Vec<u8>, StoreValue>, expiration_heap: ExpirationHeap) -> Store {
        let new_store = Store {
            hashmap: Arc::new(RwLock::new(hashmap)),
            expiration_heap: Arc::new(RwLock::new(expiration_heap)),
            wakeup: Arc::new(Notify::new()),
        };
        let sweep_store = new_store.clone();
        spawn(async move {
            sweep_store.sweep_loop().await;
        });
        new_store
    }

    async fn sweep_loop(&self) -> () {
        loop {
            self.sweep_expired_once().await;
            let next_expire = {
                let heap = self.expiration_heap.read().await;
                heap.peek().map(|Reverse((wake_time, _))| *wake_time)
            };
            match next_expire {
                None => self.wakeup.notified().await,
                Some(wake_time) => tokio::select! {
                    _ = sleep_until(wake_time) => {}
                    _ = self.wakeup.notified() => {}
                },
            }
        }
    }

    async fn sweep_expired_once(&self) -> () {
        let mut heap = self.expiration_heap.write().await;
        let mut candidates = HashSet::new();
        let now = Instant::now();

        while let Some(Reverse((when, key))) = heap.peek()
            && when <= &now
        {
            candidates.insert(key.clone());
            heap.pop();
        }

        drop(heap);

        let mut map = self.hashmap.write().await;
        for key in candidates {
            if let Some(v) = map.get(&key)
                && Store::is_expired(v, now)
            {
                map.remove_entry(&key);
            }
        }
        drop(map);
    }

    /// Returns the value for `key`, or `None` if the key is missing or expired.
    pub async fn get(&self, key: &Key) -> Option<Vec<u8>> {
        let map = self.hashmap.read().await;
        let now = Instant::now();
        match map.get(key) {
            None => None,
            Some(v) if Store::is_expired(v, now) => None,
            Some(StoreValue {
                value: v,
                expiration_time: _,
            }) => Some(v.to_vec()),
        }
    }

    /// Sets `key` to `value`, returning the previous value if one existed.
    ///
    /// Any existing expiration on the key is cleared.
    pub async fn set(&self, key: Key, value: Vec<u8>) -> Option<Vec<u8>> {
        let mut map = self.hashmap.write().await;
        map.insert(
            key,
            StoreValue {
                value: value.clone(),
                expiration_time: None,
            },
        )
        .map(
            |StoreValue {
                 value: v,
                 expiration_time: _,
             }| v.to_vec(),
        )
    }

    /// Deletes `key`, returning the stored value if it existed and was not expired.
    ///
    /// Expired keys are treated as absent.
    pub async fn del(&self, key: &Key) -> Option<Vec<u8>> {
        let now = Instant::now();
        let mut map = self.hashmap.write().await;
        match map.remove(key) {
            Some(v) if Store::is_expired(&v, now) => None,

            None => None,

            Some(StoreValue { value, .. }) => Some(value.to_vec()),
        }
    }

    /// Sets a timeout in seconds on `key`.
    ///
    /// Returns `1` if the timeout was set, or `0` if the key does not exist
    /// or is already expired.
    pub async fn expire(&self, key: Key, ttl: u64) -> u64 {
        let mut heap = self.expiration_heap.write().await;
        let mut map = self.hashmap.write().await;
        let now = Instant::now();
        let ttl_duration = Duration::new(ttl, 0);
        match map.remove_entry(&key) {
            Some(v) if Store::is_expired(&v.1, now) => 0,

            None => 0,

            Some((k, store_value)) => {
                let expires = now + ttl_duration;
                map.insert(
                    k.clone(),
                    StoreValue {
                        expiration_time: Some(expires),
                        ..store_value
                    },
                );
                heap.push(Reverse((expires, k)));
                self.wakeup.notify_one();
                1
            }
        }
    }

    /// Returns the remaining time to live for `key` in whole seconds.
    ///
    /// Returns:
    /// - `-2` if the key does not exist or is expired
    /// - `-1` if the key exists but has no expiration
    /// - a non-negative number for the remaining TTL
    pub async fn ttl(&self, key: Key) -> i64 {
        let map = self.hashmap.read().await;
        let now = Instant::now();
        match map.get(key.as_slice()) {
            None => -2,
            Some(v) if Store::is_expired(v, now) => -2,
            Some(StoreValue {
                value: _,
                expiration_time: None,
            }) => -1,
            Some(StoreValue {
                value: _,
                expiration_time: Some(expires_on),
            }) => expires_on.duration_since(now).as_secs() as i64,
        }
    }

    fn is_expired(value: &StoreValue, now: Instant) -> bool {
        matches!(value.expiration_time, Some(t) if t <= now)
    }

    async fn to_snapshot(&self) -> Snapshot {
        let now = Instant::now();
        Snapshot {
            entries: self
                .hashmap
                .read()
                .await
                .iter()
                .filter(|(_, v)| !Store::is_expired(v, now))
                .map(|(key, value)| SnapshotEntry {
                    key: key.clone(),
                    value: value.into(),
                })
                .collect(),
        }
    }

    async fn from_snapshot(snapshot: Snapshot) -> Result<Store, SnapshotError> {
        let now_unix_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System Time is set before Unix Epoch")
            .as_millis();

        let mut unique_keys: HashSet<Vec<u8>> = HashSet::new();
        for entry in snapshot.entries.iter() {
            if !unique_keys.insert(entry.key.clone()) {
                return Err(SnapshotError::DuplicateKey);
            }
        }

        let hashmap: HashMap<Vec<u8>, StoreValue> = snapshot
            .entries
            .into_iter()
            .filter(|snapshot_entry| {
                snapshot_entry.value.expiration_time_unix.is_none()
                    || snapshot_entry.value.expiration_time_unix.unwrap() > now_unix_millis
            })
            .map(
                |SnapshotEntry { key, value }| -> Result<(Vec<u8>, StoreValue), SnapshotError> {
                    Ok((key, value.try_into()?))
                },
            )
            .collect::<Result<_, _>>()?;
        let expiration_heap: ExpirationHeap = hashmap
            .iter()
            .filter_map(|(key, store_value)| match store_value {
                StoreValue {
                    expiration_time: None,
                    ..
                } => None,
                StoreValue {
                    expiration_time: Some(expiration_instant),
                    ..
                } => Some(Reverse((*expiration_instant, key.clone()))),
            })
            .collect();
        Ok(Store::from_parts(hashmap, expiration_heap))
    }

    pub async fn dump(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.to_snapshot().await)
    }

    pub async fn restore(bytes: &[u8]) -> Result<Store, RestoreError> {
        let snapshot: Snapshot = serde_json::from_slice(bytes)?;
        Store::from_snapshot(snapshot).await.map_err(Into::into)
    }
}

impl Default for Store {
    fn default() -> Self {
        Store::new()
    }
}

#[derive(Debug, Clone)]
struct StoreValue {
    value: Vec<u8>,
    expiration_time: Option<Instant>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct SnapshotValue {
    #[serde(with = "serde_bytes")]
    value: Vec<u8>,
    expiration_time_unix: Option<u128>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct SnapshotEntry {
    #[serde(with = "serde_bytes")]
    key: Vec<u8>,
    value: SnapshotValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    entries: Vec<SnapshotEntry>,
}

#[derive(Debug)]
pub enum SnapshotError {
    DurationOverflow,
    DuplicateKey,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SnapshotError::DurationOverflow => {
                write!(f, "snapshot expiration exceeds supported duration")
            }
            SnapshotError::DuplicateKey => {
                write!(f, "snapshot contains duplicate keys")
            }
        }
    }
}

impl std::error::Error for SnapshotError {}

#[derive(Debug)]
pub enum RestoreError {
    InvalidSnapshot(serde_json::Error),
    InvalidData(SnapshotError),
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestoreError::InvalidSnapshot(err) => write!(f, "{err}"),
            RestoreError::InvalidData(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for RestoreError {}

impl From<serde_json::Error> for RestoreError {
    fn from(value: serde_json::Error) -> Self {
        RestoreError::InvalidSnapshot(value)
    }
}

impl From<SnapshotError> for RestoreError {
    fn from(value: SnapshotError) -> Self {
        RestoreError::InvalidData(value)
    }
}

impl From<StoreValue> for SnapshotValue {
    fn from(store_value: StoreValue) -> Self {
        let StoreValue {
            value,
            expiration_time,
        } = store_value;
        let unix_now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System Time is set before Unix Epoch")
            .as_millis();
        let store_now = Instant::now();
        Self {
            value,
            expiration_time_unix: expiration_time
                .map(|t| t.saturating_duration_since(store_now).as_millis() + unix_now_millis),
        }
    }
}

impl From<&StoreValue> for SnapshotValue {
    fn from(store_value: &StoreValue) -> Self {
        let StoreValue {
            value,
            expiration_time,
        } = store_value.clone();
        let unix_now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_millis();
        let store_now = Instant::now();
        Self {
            value,
            expiration_time_unix: expiration_time
                .map(|t| t.saturating_duration_since(store_now).as_millis() + unix_now_millis),
        }
    }
}

impl TryFrom<SnapshotValue> for StoreValue {
    type Error = SnapshotError;

    fn try_from(snapshot_value: SnapshotValue) -> Result<Self, Self::Error> {
        let SnapshotValue {
            value,
            expiration_time_unix,
        } = snapshot_value;
        let unix_now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_millis();
        let store_now = Instant::now();
        Ok(Self {
            value,
            expiration_time: expiration_time_unix
                .map(|t| -> Result<Instant, SnapshotError> {
                    let remaining = t.saturating_sub(unix_now_millis);
                    let remaining_millis =
                        u64::try_from(remaining).map_err(|_| SnapshotError::DurationOverflow)?;
                    store_now
                        .checked_add(Duration::from_millis(remaining_millis))
                        .ok_or(SnapshotError::DurationOverflow)
                })
                .transpose()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Barrier;
    use tokio::time::{self, sleep};

    #[tokio::test]
    async fn set_then_get() {
        let store = Store::new();
        store
            .set("newkey".as_bytes().to_vec(), "newvalue".as_bytes().to_vec())
            .await;
        assert_eq!(
            Some("newvalue".as_bytes().to_vec()),
            store.get(&"newkey".as_bytes().to_vec()).await
        )
    }

    #[tokio::test]
    async fn get_nonexistent_key() {
        let store = Store::new();
        assert_eq!(None, store.get(&"newkey".as_bytes().to_vec()).await)
    }

    #[tokio::test]
    async fn delete_existing_key() {
        let store = Store::new();
        store
            .set("newkey".as_bytes().to_vec(), "newvalue".as_bytes().to_vec())
            .await;
        assert!(store.del(&"newkey".as_bytes().to_vec()).await.is_some())
    }

    #[tokio::test]
    async fn delete_nonexistent_key() {
        let store = Store::new();
        assert!(store.del(&"newkey".as_bytes().to_vec()).await.is_none())
    }

    #[tokio::test]
    async fn get_returns_none_for_expired_key() {
        let store = Store::new();
        let key = b"expiring-key".to_vec();
        let value = b"value".to_vec();

        store.set(key.clone(), value).await;
        assert_eq!(store.expire(key.clone(), 0).await, 1);

        sleep(Duration::from_millis(1)).await;

        assert_eq!(None, store.get(&key).await);
    }

    #[tokio::test]
    async fn del_returns_none_for_expired_key() {
        let store = Store::new();
        let key = b"expiring-key".to_vec();
        let value = b"value".to_vec();

        store.set(key.clone(), value).await;
        assert_eq!(store.expire(key.clone(), 0).await, 1);

        sleep(Duration::from_millis(1)).await;

        assert_eq!(None, store.del(&key).await);
    }

    #[tokio::test]
    async fn expire_returns_one_for_live_key() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();

        store.set(key.clone(), b"value".to_vec()).await;

        assert_eq!(1, store.expire(key, 60).await);
    }

    #[tokio::test]
    async fn expire_returns_zero_for_expired_key() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();

        store.set(key.clone(), b"value".to_vec()).await;
        assert_eq!(1, store.expire(key.clone(), 0).await);

        sleep(Duration::from_millis(1)).await;

        assert_eq!(0, store.expire(key, 60).await);
    }

    #[tokio::test]
    async fn ttl_returns_neg2_for_missing_key() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();

        assert_eq!(-2, store.ttl(key).await);
    }

    #[tokio::test]
    async fn ttl_returns_neg2_for_expired_key() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();

        store.set(key.clone(), b"value".to_vec()).await;

        store.expire(key.clone(), 0).await;

        sleep(Duration::from_millis(1)).await;

        assert_eq!(-2, store.ttl(key).await);
    }

    #[tokio::test]
    async fn ttl_returns_neg1_for_key_w_no_expire_time() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();
        let value = b"ttl-value".to_vec();

        store.set(key.clone(), value).await;

        assert_eq!(-1, store.ttl(key).await);
    }

    #[tokio::test]
    async fn ttl_returns_non_negative_secs_for_key_with_remaining_time() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();

        store.set(key.clone(), b"value".to_vec()).await;

        store.expire(key.clone(), 100).await;

        sleep(Duration::from_millis(1)).await;

        assert!(1 < store.ttl(key).await);
    }

    #[tokio::test]
    async fn ttl_decreases_as_time_passes() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();

        store.set(key.clone(), b"value".to_vec()).await;

        store.expire(key.clone(), 100).await;

        sleep(Duration::from_millis(1)).await;

        let tick1 = store.ttl(key.clone()).await;
        sleep(Duration::from_secs(1)).await;
        let tick2 = store.ttl(key).await;

        assert!(tick1 > tick2);
    }

    #[tokio::test(start_paused = true)]
    async fn setting_an_existing_key_with_ttl_clears_the_ttl() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();
        let replacement_value = b"new-value".to_vec();

        store.set(key.clone(), b"value".to_vec()).await;
        assert_eq!(1, store.expire(key.clone(), 60).await);
        assert_eq!(
            Some(b"value".to_vec()),
            store.set(key.clone(), replacement_value.clone()).await
        );

        tokio::time::advance(Duration::from_secs(60)).await;
        store.sweep_expired_once().await;

        assert_eq!(Some(replacement_value), store.get(&key).await);
        assert_eq!(store.ttl(key).await, -1)
    }

    #[tokio::test(start_paused = true)]
    async fn stale_heap_entry_does_not_delete_reexpired_key() {
        let store = Store::new();
        let key = b"ttl-key".to_vec();
        let value = b"value".to_vec();

        store.set(key.clone(), value.clone()).await;
        assert_eq!(1, store.expire(key.clone(), 5).await);
        assert_eq!(1, store.expire(key.clone(), 10).await);

        tokio::time::advance(Duration::from_secs(5)).await;
        store.sweep_expired_once().await;

        assert_eq!(Some(value.clone()), store.get(&key).await);
        assert!(store.ttl(key.clone()).await > 0);

        tokio::time::advance(Duration::from_secs(5)).await;
        store.sweep_expired_once().await;

        assert_eq!(None, store.get(&key).await);
        assert_eq!(-2, store.ttl(key).await);
    }

    #[tokio::test]
    async fn sweep_once_clears_all_expired_keys() {
        let store = Store::new();
        for i in 0..10 {
            let key: Vec<u8> = u8::to_le_bytes(i).to_vec();
            store.set(key.clone(), b"value".to_vec()).await;
            store.expire(key, 0).await;
        }
        let persistent_key = b"persistent_key".to_vec();
        store
            .set(persistent_key.clone(), b"this key should remain".to_vec())
            .await;
        sleep(Duration::from_millis(1)).await;
        store.sweep_expired_once().await;
        let map = store.hashmap.read().await;
        for i in 0..10 {
            let key: Vec<u8> = u8::to_le_bytes(i).to_vec();
            assert!(!map.contains_key(&key))
        }
        assert!(map.contains_key(&persistent_key));
        let heap = store.expiration_heap.read().await;
        assert_eq!(0, heap.len())
    }
    #[tokio::test]
    async fn sweep_loop_clears_all_expired_keys() {
        let store = Store::new();
        for i in 0..10 {
            let key: Vec<u8> = u8::to_le_bytes(i).to_vec();
            store.set(key.clone(), b"value".to_vec()).await;
            store.expire(key, 0).await;
        }
        let persistent_key = b"persistent_key".to_vec();
        store
            .set(persistent_key.clone(), b"this key should remain".to_vec())
            .await;
        sleep(Duration::from_millis(1)).await;
        let map = store.hashmap.read().await;
        for i in 0..10 {
            let key: Vec<u8> = u8::to_le_bytes(i).to_vec();
            assert!(!map.contains_key(&key))
        }
        assert!(map.contains_key(&persistent_key));
        let heap = store.expiration_heap.read().await;
        assert_eq!(0, heap.len())
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_expires() {
        let barrier = Arc::new(Barrier::new(3));

        let store = Store::new();
        let key = b"ttl-key".to_vec();
        store.set(key.clone(), b"my_val".to_vec()).await;

        let task_a = {
            let store = store.clone();
            let key = key.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                store.expire(key, 5).await
            })
        };

        let task_b = {
            let store = store.clone();
            let key = key.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                store.expire(key, 10).await
            })
        };

        barrier.wait().await;
        let a = task_a.await.unwrap();
        let b = task_b.await.unwrap();

        assert_eq!(1, a);
        assert_eq!(1, b);

        tokio::time::advance(Duration::from_secs(5)).await;
        store.sweep_expired_once().await;

        let after_five = store.get(&key).await;

        tokio::time::advance(Duration::from_secs(5)).await;
        store.sweep_expired_once().await;

        let after_ten = store.get(&key).await;

        assert!(after_five.is_none() || after_ten.is_none());
        assert_eq!(None, after_ten)
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_del_vs_ttl() {
        let barrier = Arc::new(Barrier::new(3));

        let store = Store::new();
        let key = b"ttl-key".to_vec();
        store.set(key.clone(), b"my_val".to_vec()).await;

        let task_a = {
            let store = store.clone();
            let key = key.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                store.expire(key, 0).await
            })
        };

        let task_b = {
            let store = store.clone();
            let key = key.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                store.del(&key).await
            })
        };

        barrier.wait().await;

        let a = task_a.await.unwrap();
        let b = task_b.await.unwrap();

        assert!(a == 0 || a == 1);
        assert!(b == Some(b"my_val".to_vec()) || b.is_none());

        store.sweep_expired_once().await;

        assert_eq!(None, store.get(&key).await);
        assert_eq!(-2, store.ttl(key).await);
    }

    #[test]
    fn snapshot_from_store_value_preserves_value_and_none_expiration() {
        let store_value = StoreValue {
            value: b"snapshot-value".to_vec(),
            expiration_time: None,
        };

        let snapshot_value: SnapshotValue = store_value.into();

        assert_eq!(snapshot_value.value, b"snapshot-value".to_vec());
        assert_eq!(snapshot_value.expiration_time_unix, None);
    }

    #[test]
    fn store_value_from_snapshot_preserves_value_and_none_expiration() {
        let snapshot_value = SnapshotValue {
            value: b"snapshot-value".to_vec(),
            expiration_time_unix: None,
        };

        let store_value: StoreValue = snapshot_value.try_into().expect("valid snapshot value");

        assert_eq!(store_value.value, b"snapshot-value".to_vec());
        assert_eq!(store_value.expiration_time, None);
    }

    #[test]
    fn snapshot_from_store_value_converts_future_expiration_to_unix_time() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_millis();
        let store_value = StoreValue {
            value: b"snapshot-value".to_vec(),
            expiration_time: Some(Instant::now() + Duration::from_secs(5)),
        };

        let snapshot_value: SnapshotValue = store_value.into();

        let expiration_time_unix = snapshot_value
            .expiration_time_unix
            .expect("Expected expiration time");
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_millis();

        assert!(((before + 4_000)..=(after + 5_000)).contains(&expiration_time_unix));
    }

    #[test]
    fn store_value_from_snapshot_converts_future_unix_time_to_instant() {
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_millis();
        let snapshot_value = SnapshotValue {
            value: b"snapshot-value".to_vec(),
            expiration_time_unix: Some(now_unix + 5_000),
        };
        let before = Instant::now();

        let store_value: StoreValue = snapshot_value.try_into().expect("valid snapshot value");

        let after = Instant::now();
        let expiration_time = store_value
            .expiration_time
            .expect("Expected expiration time");

        assert_eq!(store_value.value, b"snapshot-value".to_vec());
        assert!(expiration_time >= before + Duration::from_secs(4));
        assert!(expiration_time <= after + Duration::from_secs(5));
    }

    #[tokio::test]
    async fn from_snapshot_restores_persistent_and_future_entries_only() {
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_millis();
        let snapshot = Snapshot {
            entries: vec![
                SnapshotEntry {
                    key: b"persistent-key".to_vec(),
                    value: SnapshotValue {
                        value: b"persistent-value".to_vec(),
                        expiration_time_unix: None,
                    },
                },
                SnapshotEntry {
                    key: b"future-key".to_vec(),
                    value: SnapshotValue {
                        value: b"future-value".to_vec(),
                        expiration_time_unix: Some(now_unix + 60_000),
                    },
                },
                SnapshotEntry {
                    key: b"expired-key".to_vec(),
                    value: SnapshotValue {
                        value: b"expired-value".to_vec(),
                        expiration_time_unix: Some(now_unix.saturating_sub(1)),
                    },
                },
            ],
        };

        let store = Store::from_snapshot(snapshot)
            .await
            .expect("valid snapshot");

        assert_eq!(
            Some(b"persistent-value".to_vec()),
            store.get(&b"persistent-key".to_vec()).await
        );
        assert_eq!(
            Some(b"future-value".to_vec()),
            store.get(&b"future-key".to_vec()).await
        );
        assert_eq!(None, store.get(&b"expired-key".to_vec()).await);
        assert_eq!(-1, store.ttl(b"persistent-key".to_vec()).await);

        let future_ttl = store.ttl(b"future-key".to_vec()).await;
        assert!((0..=60).contains(&future_ttl));
        assert_eq!(-2, store.ttl(b"expired-key".to_vec()).await);
    }

    #[tokio::test]
    async fn snapshot_round_trip_restores_live_entries() {
        let store = Store::new();
        let persistent_key = b"roundtrip-persistent".to_vec();
        let expiring_key = b"roundtrip-expiring".to_vec();
        let expired_key = b"roundtrip-expired".to_vec();

        store
            .set(persistent_key.clone(), b"persistent-value".to_vec())
            .await;
        store
            .set(expiring_key.clone(), b"future-value".to_vec())
            .await;
        store
            .set(expired_key.clone(), b"expired-value".to_vec())
            .await;

        assert_eq!(1, store.expire(expiring_key.clone(), 60).await);
        assert_eq!(1, store.expire(expired_key.clone(), 0).await);

        sleep(Duration::from_millis(1)).await;

        let snapshot = store.to_snapshot().await;
        let restored = Store::from_snapshot(snapshot)
            .await
            .expect("valid snapshot");

        assert_eq!(
            Some(b"persistent-value".to_vec()),
            restored.get(&persistent_key).await
        );
        assert_eq!(
            Some(b"future-value".to_vec()),
            restored.get(&expiring_key).await
        );
        assert_eq!(None, restored.get(&expired_key).await);
        assert_eq!(-1, restored.ttl(persistent_key).await);

        let future_ttl = restored.ttl(expiring_key).await;
        assert!((0..=60).contains(&future_ttl));
        assert_eq!(-2, restored.ttl(expired_key).await);
    }

    #[tokio::test]
    async fn restore_invalid_json_returns_err() {
        assert!(Store::restore(b"{").await.is_err())
    }

    #[test]
    fn store_value_from_snapshot_rejects_unrepresentable_duration() {
        let snapshot_value = SnapshotValue {
            value: b"snapshot-value".to_vec(),
            expiration_time_unix: Some(u128::MAX),
        };

        assert!(matches!(
            StoreValue::try_from(snapshot_value),
            Err(SnapshotError::DurationOverflow)
        ));
    }

    #[tokio::test]
    async fn truncated_archive_is_rejected_by_restore() {
        let s = Store::new();
        for i in 0u8..3 {
            s.set(i.to_le_bytes().to_vec(), b"my_value".to_vec()).await;
        }
        let truncated_dump = s.dump().await.unwrap();
        for i in 1..=5 {
            assert!(matches!(
                Store::restore(
                    truncated_dump.as_slice()[..truncated_dump.len() - i]
                        .iter()
                        .as_slice()
                )
                .await,
                Err(RestoreError::InvalidSnapshot(_))
            ));
        }
    }

    #[tokio::test]
    async fn view_json_dump() {
        let s: Store = Store::new();
        for i in 0u8..3 {
            s.set(i.to_le_bytes().to_vec(), b"my_value".to_vec()).await;
        }
        let archive = s.dump().await.unwrap();
        println!("{}", String::from_utf8(archive).unwrap());
    }

    #[tokio::test]
    async fn restore_rejects_unix_timestamp_overflow() {
        let archive = br#"{"entries":[{"key":[0],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":340282366920938463463374607431768211455}},{"key":[1],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}},{"key":[2],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}}]}"#;
        assert!(matches!(
            Store::restore(archive).await,
            Err(RestoreError::InvalidData(_))
        ));
    }

    #[tokio::test]
    async fn restore_of_archives_with_duplicate_keys_fails() {
        let archive = br#"{"entries":[{"key":[0],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}},{"key":[0],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}},{"key":[2],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}}]}"#;
        assert!(matches!(
            Store::restore(archive).await,
            Err(RestoreError::InvalidData(SnapshotError::DuplicateKey))
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn dump_excludes_expired_entries() {
        let s = Store::new();
        s.set(b"live_key".to_vec(), b"live_value".to_vec()).await;
        s.expire(b"live_key".to_vec(), 1000).await;
        s.set(b"expired_key".to_vec(), b"expired_value".to_vec())
            .await;
        s.expire(b"expired_key".to_vec(), 0).await;
        s.set(b"persistent_key".to_vec(), b"persistent_value".to_vec())
            .await;
        let bytes = s.dump().await.unwrap();
        let s = Store::restore(&bytes).await.unwrap();
        time::advance(Duration::from_secs(5)).await;
        assert_eq!(
            s.get(&b"live_key".to_vec()).await.unwrap(),
            b"live_value".to_vec()
        );
        assert!(s.get(&b"expired_key".to_vec()).await.is_none());
        assert_eq!(
            s.get(&b"persistent_key".to_vec()).await.unwrap(),
            b"persistent_value".to_vec()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn binary_values_preserved_e2e() {
        let s = Store::new();
        s.set(b"empty_bytes_key".to_vec(), b"".to_vec()).await;
        s.set(b"non_utf_bytes_key".to_vec(), b"\xF4\xFF".to_vec())
            .await;
        s.set(b"embedded_zero_key".to_vec(), b"hello\x00world".to_vec())
            .await;
        s.set(b"\xF4\xFF".to_vec(), b"value".to_vec()).await;
        let bytes = s.dump().await.unwrap();
        let s = Store::restore(&bytes).await.unwrap();
        time::advance(Duration::from_secs(5)).await;
        assert_eq!(
            s.get(&b"empty_bytes_key".to_vec()).await.unwrap(),
            b"".to_vec()
        );
        assert_eq!(
            s.get(&b"non_utf_bytes_key".to_vec()).await.unwrap(),
            b"\xF4\xFF".to_vec()
        );
        assert_eq!(
            s.get(&b"embedded_zero_key".to_vec()).await.unwrap(),
            b"hello\x00world".to_vec()
        );
        assert_eq!(
            s.get(&b"\xF4\xFF".to_vec()).await.unwrap(),
            b"value".to_vec()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expiration_times_properly_preserved_e2e() {
        let s = Store::new();
        s.set(b"live_key".to_vec(), b"live_value".to_vec()).await;
        s.expire(b"live_key".to_vec(), 5).await;
        let bytes = s.dump().await.unwrap();
        let s = Store::restore(&bytes).await.unwrap();
        assert_eq!(
            s.get(&b"live_key".to_vec()).await.unwrap(),
            b"live_value".to_vec()
        );
        time::advance(Duration::from_secs(5)).await;
        assert!(s.get(&b"live_key".to_vec()).await.is_none());
    }

    #[tokio::test]
    async fn json_with_missing_fields_fails() {
        let archive = br#"{"entries":[{"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}},{"key":[1],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}},{"key":[2],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}}]}"#;
        assert!(matches!(
            Store::restore(archive).await,
            Err(RestoreError::InvalidSnapshot(_))
        ));
    }

    #[tokio::test]
    async fn json_with_unrecognized_fields_fails() {
        let archive = br#"{"entries":[{"key":[0],"unrecognized":[0],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}},{"key":[1],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}},{"key":[2],"value":{"value":[109,121,95,118,97,108,117,101],"expiration_time_unix":null}}]}"#;
        assert!(matches!(
            Store::restore(archive).await,
            Err(RestoreError::InvalidSnapshot(_))
        ));
    }

    #[tokio::test]
    async fn archive_with_no_entries_creates_empty_store() {
        let archive = br#"{"entries":[]}"#;
        let s = Store::restore(archive).await.unwrap();
        assert_eq!(s.hashmap.read().await.len(), 0)
    }
}
