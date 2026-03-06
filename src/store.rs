use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, PartialEq, Debug)]
pub enum RedisValue {
    RedisString(Vec<u8>),
    RedisInteger(i64),
    RedisArray(Vec<RedisValue>),
}

pub struct Store {
    inner: Arc<RwLock<HashMap<String, RedisValue>>>,
}

impl Store {
    pub fn new() -> Store {
        Store {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, key: &str) -> Option<RedisValue> {
        let map = self.inner.read().await;
        map.get(key).cloned()
    }

    pub async fn set(&self, key: String, value: RedisValue) -> Option<RedisValue> {
        let mut map = self.inner.write().await;
        map.insert(key, value)
    }

    pub async fn del(&self, key: &str) -> Option<RedisValue> {
        let mut map = self.inner.write().await;
        map.remove(key)
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
        let newval = RedisValue::RedisString(b"newvalue".to_vec());
        store.set("newkey".to_string(), newval).await;
        assert_eq!(
            Some(RedisValue::RedisString(b"newvalue".to_vec())),
            store.get("newkey").await
        )
    }

    #[tokio::test]
    async fn get_nonexistent_key() {
        let store = Store::new();
        assert_eq!(None, store.get("newkey").await)
    }

    #[tokio::test]
    async fn delete_existing_key() {
        let store = Store::new();
        let newval = RedisValue::RedisString(b"newvalue".to_vec());
        store.set("newkey".to_string(), newval).await;
        assert!(store.del("newkey").await.is_some())
    }

    #[tokio::test]
    async fn delete_nonexistent_key() {
        let store = Store::new();
        assert!(store.del("newkey").await.is_none())
    }
}
