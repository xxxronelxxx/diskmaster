use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::duplicates::DuplicateState;
use crate::guard::AllowList;
use crate::types::{InstalledProgram, ScanState, WingetListing};

pub struct ProgramCache {
    pub at: Instant,
    pub items: Vec<InstalledProgram>,
}

pub struct UpdateCache {
    pub at: Instant,
    pub listing: WingetListing,
}

#[derive(Clone)]
pub struct AppState {
    pub scan: Arc<Mutex<Option<ScanState>>>,
    pub scan_cancel: Arc<std::sync::atomic::AtomicBool>,
    pub allow: Arc<Mutex<AllowList>>,
    pub duplicates: Arc<Mutex<DuplicateState>>,
    pub programs: Arc<Mutex<Option<ProgramCache>>>,
    pub updates: Arc<Mutex<Option<UpdateCache>>>,
    pub startup_ids: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            scan: Arc::new(Mutex::new(None)),
            scan_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            allow: Arc::new(Mutex::new(AllowList::new())),
            duplicates: Arc::new(Mutex::new(DuplicateState::default())),
            programs: Arc::new(Mutex::new(None)),
            updates: Arc::new(Mutex::new(None)),
            startup_ids: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
