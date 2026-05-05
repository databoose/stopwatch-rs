use std::{collections::VecDeque};
use std::sync::{Mutex, LazyLock};

pub struct DebugLog {
    debug_msg_list: VecDeque<String>,
    capacity: usize,
    count: u8 // number to show in which place that debug log entry occured
}

impl DebugLog {
    fn new(capacity: usize) -> Self {
        Self {
            debug_msg_list: VecDeque::new(),
            capacity,
            count: 0
        }
    }
    
    pub fn log(msg: &str) {
        if let Ok(mut lock) = LOGGER.lock() {
            if lock.debug_msg_list.len() >= lock.capacity {
                lock.debug_msg_list.pop_front();
            }
            lock.count = lock.count.saturating_add(1);
            
            let display_number = lock.count;
            lock.debug_msg_list.push_back(format!("[{}] {}", display_number, msg));
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
    Mutex::new(DebugLog::new(40))
});