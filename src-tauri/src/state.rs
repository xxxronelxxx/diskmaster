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
    pub allow: Arc<Mutex<AllowList>>,
    pub duplicates: Arc<Mutex<DuplicateState>>,
    pub programs: Arc<Mutex<Option<ProgramCache>>>,
    pub updates: Arc<Mutex<Option<UpdateCache>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            scan: Arc::new(Mutex::new(None)),
            allow: Arc::new(Mutex::new(AllowList::new())),
            duplicates: Arc::new(Mutex::new(DuplicateState::default())),
            programs: Arc::new(Mutex::new(None)),
            updates: Arc::new(Mutex::new(None)),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
