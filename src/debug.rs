use std::{collections::VecDeque, fmt::Debug};
use std::sync::{Mutex, LazyLock};

pub struct DebugLog {
    debug_msg_list: VecDeque<String>,
    capacity: usize
}

impl DebugLog {
    fn new(capacity: usize) -> Self {
        Self {
            debug_msg_list: VecDeque::new(),
            capacity
        }
    }
    
    pub fn log(msg: &str) {
        if let Ok(mut lock) = LOGGER.lock() {
            if lock.debug_msg_list.len() >= lock.capacity {
                lock.debug_msg_list.pop_front();
                lock.debug_msg_list.push_back(msg.to_string());
            }
            lock.debug_msg_list.push_back(msg.to_string());
        }
    }
    
    pub fn get_all() -> Vec<String> {
        if let Ok(lock) = LOGGER.lock() {
            return lock.debug_msg_list.iter().cloned().collect();
        }
        vec![]
    }
    
}

pub static LOGGER: LazyLock<Mutex<DebugLog>> = LazyLock::new(|| {
    Mutex::new(DebugLog::new(20))
});