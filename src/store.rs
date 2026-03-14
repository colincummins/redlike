use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

type Key = Vec<u8>;
type ExpirationEntry = Reverse<(Instant, Key)>;
type ExpirationHeap = BinaryHeap<ExpirationEntry>;

pub struct Store {
    hashmap: Arc<RwLock<HashMap<Vec<u8>, StoreValue>>>,
    expiration_heap: Arc<RwLock<ExpirationHeap>>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct StoreValue {
    value: Vec<u8>,
    expiration_time: Option<Instant>,
}

impl Store {
    fn is_expired(&self, value: &StoreValue, now: Instant) -> bool {
        matches!(value.expiration_time, Some(t) if t <= now)
    }
    pub fn new() -> Store {
        Store {
            hashmap: Arc::new(RwLock::new(HashMap::new())),
            expiration_heap: Arc::new(RwLock::new(BinaryHeap::new())),
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
            match map.get(&key).await {}
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
        let mut map = self.hashmap.write().await;
        let now = Instant::now();
        let ttl_duration = Duration::new(ttl, 0);
        match map.remove_entry(&key) {
            Some(v) if self.is_expired(&v.1, now) => 0,

            None => 0,

            Some((k, store_value)) => {
                map.insert(
                    k,
                    StoreValue {
                        expiration_time: Some(now + ttl_duration),
                        ..store_value
                    },
                );
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
}
