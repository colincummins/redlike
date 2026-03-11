use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Store {
    inner: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
}

impl Store {
    pub fn new() -> Store {
        Store {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, key: &Vec<u8>) -> Option<Vec<u8>> {
        let map = self.inner.read().await;
        map.get(key).cloned()
    }

    pub async fn set(&self, key: Vec<u8>, value: Vec<u8>) -> Option<Vec<u8>> {
        let mut map = self.inner.write().await;
        map.insert(key, value)
    }

    pub async fn del(&self, key: &Vec<u8>) -> Option<Vec<u8>> {
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
