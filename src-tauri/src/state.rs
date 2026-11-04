use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::duplicates::DuplicateState;
use crate::guard::AllowList;
use crate::types::{InstalledProgram, ScanState, WidgetSettings, WingetListing};

pub struct ProgramCache {
    pub at: Instant,
    pub items: Vec<InstalledProgram>,
}

pub struct UpdateCache {
    pub at: Instant,
    pub listing: WingetListing,
}

pub struct WidgetRuntime {
    pub child: Option<Child>,
    pub last_error: Option<String>,
    pub settings: Option<WidgetSettings>,
}

impl Default for WidgetRuntime {
    fn default() -> Self {
        Self {
            child: None,
            last_error: None,
            settings: None,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub scan: Arc<Mutex<Option<ScanState>>>,
    pub allow: Arc<Mutex<AllowList>>,
    pub duplicates: Arc<Mutex<DuplicateState>>,
    pub programs: Arc<Mutex<Option<ProgramCache>>>,
    pub updates: Arc<Mutex<Option<UpdateCache>>>,
    pub widget: Arc<Mutex<WidgetRuntime>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            scan: Arc::new(Mutex::new(None)),
            allow: Arc::new(Mutex::new(AllowList::new())),
            duplicates: Arc::new(Mutex::new(DuplicateState::default())),
            programs: Arc::new(Mutex::new(None)),
            updates: Arc::new(Mutex::new(None)),
            widget: Arc::new(Mutex::new(WidgetRuntime::default())),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
