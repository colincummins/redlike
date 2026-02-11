use std::collections::HashMap;
use std::default;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Store {
    inner: Arc<RwLock<HashMap<String, String>>>
}

impl Store {
    pub fn new() -> Store{
        Store { inner: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn get(&self, key: &str) -> Option<String>{
        let map = self.inner.read().await;
        map.get(key).cloned()
    }

    pub async fn set(&self, key: String, value: String) -> Option<String>{
        let mut map = self.inner.write().await;
        map.insert(key, value)
    }

    pub async fn delete(&self, key: &str) -> Option<String>{
        let mut map = self.inner.write().await;
        map.remove(key)
    }

} 

impl Clone for Store {
    fn clone(&self) -> Self {
        Store {
            inner: self.inner.clone()
        }
    }
}

impl Default for Store {
    fn default() -> Self {
        Store::new()
    }
}
