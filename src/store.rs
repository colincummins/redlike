use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use tokio::spawn;
use tokio::sync::{Notify, RwLock};
use tokio::time::{Duration, Instant, sleep_until};

type Key = Vec<u8>;
type ExpirationEntry = Reverse<(Instant, Key)>;
type ExpirationHeap = BinaryHeap<ExpirationEntry>;

pub struct Store {
    hashmap: Arc<RwLock<HashMap<Vec<u8>, StoreValue>>>,
    expiration_heap: Arc<RwLock<ExpirationHeap>>,
    wakeup: Arc<Notify>,
}

#[derive(Debug, Clone)]
struct StoreValue {
    value: Vec<u8>,
    expiration_time: Option<Instant>,
}

impl Store {
    fn is_expired(&self, value: &StoreValue, now: Instant) -> bool {
        matches!(value.expiration_time, Some(t) if t <= now)
    }
    pub fn new() -> Store {
        let new_store = Store {
            hashmap: Arc::new(RwLock::new(HashMap::new())),
            expiration_heap: Arc::new(RwLock::new(BinaryHeap::new())),
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
                && self.is_expired(v, now)
            {
                map.remove_entry(&key);
            }
        }
        drop(map);
    }

    pub async fn get(&self, key: &Key) -> Option<Vec<u8>> {
        let map = self.hashmap.read().await;
        let now = Instant::now();
        match map.get(key) {
            None => None,
            Some(v) if self.is_expired(v, now) => None,
            Some(StoreValue {
                value: v,
                expiration_time: _,
            }) => Some(v.to_vec()),
        }
    }

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

    pub async fn del(&self, key: &Key) -> Option<Vec<u8>> {
        let now = Instant::now();
        let mut map = self.hashmap.write().await;
        match map.remove(key) {
            Some(v) if self.is_expired(&v, now) => None,

            None => None,

            Some(StoreValue { value, .. }) => Some(value.to_vec()),
        }
    }

    pub async fn expire(&self, key: Key, ttl: u64) -> u64 {
        let mut heap = self.expiration_heap.write().await;
        let mut map = self.hashmap.write().await;
        let now = Instant::now();
        let ttl_duration = Duration::new(ttl, 0);
        match map.remove_entry(&key) {
            Some(v) if self.is_expired(&v.1, now) => 0,

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

    pub async fn ttl(&self, key: Key) -> i64 {
        let map = self.hashmap.read().await;
        let now = Instant::now();
        match map.get(key.as_slice()) {
            None => -2,
            Some(v) if self.is_expired(v, now) => -2,
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
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Store {
            hashmap: self.hashmap.clone(),
            expiration_heap: self.expiration_heap.clone(),
            wakeup: self.wakeup.clone(),
        }
    }
}

impl Default for Store {
    fn default() -> Self {
        Store::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
