use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type Store= Arc<RwLock<HashMap<String, String>>>;