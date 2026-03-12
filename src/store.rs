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
    pub fn new() -> Store {
        Store {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, key: &Vec<u8>) -> Option<Vec<u8>> {
        let map = self.inner.read().await;
        match map.get(key) {
            None => None,
            Some(StoreValue {
                value: v,
                expiration_time: None,
            }) => Some(v.to_vec()),
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
        let mut map = self.inner.write().await;
        map.remove(key).map(
            |StoreValue {
                 value: v,
                 expiration_time: _,
             }| v.to_vec(),
        )
    }

    pub async fn expire(&self, key: Vec<u8>, ttl: u64) -> u8 {
        let mut map = self.inner.write().await;
        let now = Instant::now();
        let ttl_duration = Duration::new(ttl, 0);
        match map.remove_entry(&key) {
            Some((
                k,
                StoreValue {
                    value,
                    expiration_time: Some(t),
                },
            )) if t <= now => 0,

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
}
