use once_cell::sync::Lazy;

use std::{collections::HashSet, sync::Mutex};

// Global keystate - accessible from anywhere
pub static KEYSTATE: Lazy<Mutex<HashSet<u16>>> = Lazy::new(|| Mutex::new(HashSet::new()));

#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Key {
    W = 13,
    A = 0,
    S = 1,
    D = 2,
    Q = 12,
    E = 14,
    SPC = 49,
    C = 8,
    R = 15,
    F = 3,
}

impl Key {
    pub fn is_pressed(self) -> bool {
        return KEYSTATE.lock().unwrap().contains(&(self as u16));
    }
}
