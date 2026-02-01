use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Store {
    inner: Arc<RwLock<HashMap<String, String>>>
}

impl Store {
    pub fn new() -> Store{
        Store { inner: Arc::new(RwLock::new(HashMap::new())) }
    }
} 

impl Clone for Store {
    fn clone(&self) -> Self {
        Store {
            inner: self.inner.clone()
        }
    }
}