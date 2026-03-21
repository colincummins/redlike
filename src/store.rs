use serde::{Deserialize, Serialize};
use serde_json::{Deserializer, Serializer, to_vec};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
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

    async fn from_snapshot(snapshot: Snapshot) -> Store {
        let now_unix_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System Time is set before Unix Epoch")
            .as_secs();
        let hashmap: HashMap<Vec<u8>, StoreValue> = snapshot
            .entries
            .into_iter()
            .filter(|snapshot_entry| {
                snapshot_entry.value.expiration_time_unix.is_none()
                    || snapshot_entry.value.expiration_time_unix.unwrap() > now_unix_seconds
            })
            .map(|SnapshotEntry { key, value }| (key, value.into()))
            .collect();
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
        Store::from_parts(hashmap, expiration_heap)
    }

    pub async fn dump(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.to_snapshot().await)
    }

    pub async fn restore(bytes: &[u8]) -> Result<Store, serde_json::Error> {
        let snapshot: Snapshot = serde_json::from_slice(bytes).unwrap();
        Ok(Store::from_snapshot(snapshot).await)
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
struct SnapshotValue {
    #[serde(with = "serde_bytes")]
    value: Vec<u8>,
    expiration_time_unix: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SnapshotEntry {
    #[serde(with = "serde_bytes")]
    key: Vec<u8>,
    value: SnapshotValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Snapshot {
    entries: Vec<SnapshotEntry>,
}

#[derive(Debug)]
struct SnapshotError;

impl From<StoreValue> for SnapshotValue {
    fn from(store_value: StoreValue) -> Self {
        let StoreValue {
            value,
            expiration_time,
        } = store_value;
        let unix_now_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System Time is set before Unix Epoch")
            .as_secs();
        let store_now = Instant::now();
        Self {
            value,
            expiration_time_unix: expiration_time
                .map(|t| t.saturating_duration_since(store_now).as_secs() + unix_now_seconds),
        }
    }
}

impl From<&StoreValue> for SnapshotValue {
    fn from(store_value: &StoreValue) -> Self {
        let StoreValue {
            value,
            expiration_time,
        } = store_value.clone();
        let unix_now_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_secs();
        let store_now = Instant::now();
        Self {
            value,
            expiration_time_unix: expiration_time
                .map(|t| t.saturating_duration_since(store_now).as_secs() + unix_now_seconds),
        }
    }
}

impl From<SnapshotValue> for StoreValue {
    fn from(snapshot_value: SnapshotValue) -> Self {
        let SnapshotValue {
            value,
            expiration_time_unix,
        } = snapshot_value;
        let unix_now_seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_secs();
        let store_now = Instant::now();
        Self {
            value,
            expiration_time: expiration_time_unix.map(|t| {
                let remaining = t.saturating_sub(unix_now_seconds);
                store_now
                    .checked_add(Duration::from_secs(remaining))
                    .expect("Unable to calculate expiration time")
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Barrier;
    use tokio::time::sleep;

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

        let store_value: StoreValue = snapshot_value.into();

        assert_eq!(store_value.value, b"snapshot-value".to_vec());
        assert_eq!(store_value.expiration_time, None);
    }

    #[test]
    fn snapshot_from_store_value_converts_future_expiration_to_unix_time() {
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_secs();
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
            .as_secs();

        assert!(((before + 4)..=(after + 5)).contains(&expiration_time_unix));
    }

    #[test]
    fn store_value_from_snapshot_converts_future_unix_time_to_instant() {
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time before UNIX epoch")
            .as_secs();
        let snapshot_value = SnapshotValue {
            value: b"snapshot-value".to_vec(),
            expiration_time_unix: Some(now_unix + 5),
        };
        let before = Instant::now();

        let store_value: StoreValue = snapshot_value.into();

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
            .as_secs();
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
                        expiration_time_unix: Some(now_unix + 60),
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

        let store = Store::from_snapshot(snapshot).await;

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
        let restored = Store::from_snapshot(snapshot).await;

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
}
