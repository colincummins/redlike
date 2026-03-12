use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

pub struct Store {
    inner: Arc<RwLock<HashMap<Vec<u8>, StoreValue>>>,
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
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, key: &Vec<u8>) -> Option<Vec<u8>> {
        let map = self.inner.read().await;
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

    pub async fn set(&self, key: Vec<u8>, value: Vec<u8>) -> Option<Vec<u8>> {
        let mut map = self.inner.write().await;
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

    pub async fn del(&self, key: &Vec<u8>) -> Option<Vec<u8>> {
        let now = Instant::now();
        let mut map = self.inner.write().await;
        match map.remove(key) {
            Some(v) if self.is_expired(&v, now) => None,

            None => None,

            Some(StoreValue { value, .. }) => Some(value.to_vec()),
        }
    }

    pub async fn expire(&self, key: Vec<u8>, ttl: u64) -> u8 {
        let mut map = self.inner.write().await;
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
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Store {
            inner: self.inner.clone(),
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
}
