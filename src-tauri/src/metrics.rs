use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sysinfo::{Disks, Networks, Pid, ProcessesToUpdate, System};

use crate::types::{MetricsSample, MetricsState, ProcessInfo};

const HISTORY_SIZE: usize = 240;
pub const SAMPLE_INTERVAL_MS: u64 = 1000;

#[derive(Default)]
struct Store {
    history: Vec<MetricsSample>,
    error: Option<String>,
}

pub struct MetricsHandle {
    store: Arc<Mutex<Store>>,
    running: Arc<AtomicBool>,
}

impl MetricsHandle {
    pub fn start() -> Self {
        let handle = Self {
            store: Arc::new(Mutex::new(Store::default())),
            running: Arc::new(AtomicBool::new(false)),
        };
        handle.spawn_sampler();
        handle
    }

    fn spawn_sampler(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let store = Arc::clone(&self.store);
        let running = Arc::clone(&self.running);

        std::thread::spawn(move || {
            let mut system = System::new();
            let mut disks = Disks::new_with_refreshed_list();
            let mut networks = Networks::new_with_refreshed_list();

            // The first CPU refresh has no previous values to compare with.
            system.refresh_cpu_usage();
            system.refresh_memory();
            let mut last = Instant::now();

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));

                let elapsed = last.elapsed().as_secs_f64().max(0.1);
                last = Instant::now();

                system.refresh_cpu_usage();
                system.refresh_memory();
                disks.refresh(true);
                networks.refresh(true);

                let cpu = system.global_cpu_usage() as f64;
                let cores: Vec<f64> = system
                    .cpus()
                    .iter()
                    .map(|core| (core.cpu_usage().round() as f64).clamp(0.0, 100.0))
                    .collect();

                let mut read_bytes = 0u64;
                let mut written_bytes = 0u64;
                for disk in disks.list() {
                    let usage = disk.usage();
                    read_bytes = read_bytes.saturating_add(usage.read_bytes);
                    written_bytes = written_bytes.saturating_add(usage.written_bytes);
                }

                let mut net_in = 0f64;
                let mut net_out = 0f64;
                let mut adapter: Option<String> = None;
                let mut best = -1f64;
                for (name, data) in networks.iter() {
                    let received = data.received() as f64 / elapsed;
                    let transmitted = data.transmitted() as f64 / elapsed;
                    net_in += received;
                    net_out += transmitted;
                    let total = received + transmitted;
                    if total > best {
                        best = total;
                        adapter = Some(name.clone());
                    }
                }

                let ram_total = system.total_memory();
                let ram_used = system.used_memory();

                let sample = MetricsSample {
                    t: chrono::Utc::now().timestamp_millis(),
                    cpu: (cpu * 10.0).round() / 10.0,
                    cores,
                    ram_used,
                    ram_total,
                    disk_read: read_bytes as f64 / elapsed,
                    disk_write: written_bytes as f64 / elapsed,
                    net_in,
                    net_out,
                    net_adapter: adapter,
                };

                let mut guard = store.lock().unwrap();
                guard.error = None;
                guard.history.push(sample);
                if guard.history.len() > HISTORY_SIZE {
                    let cut = guard.history.len() - HISTORY_SIZE;
                    guard.history.drain(0..cut);
                }
            }
        });
    }

    pub fn snapshot(&self) -> (Vec<MetricsSample>, Option<MetricsSample>, Option<String>) {
        let guard = self.store.lock().unwrap();
        (
            guard.history.clone(),
            guard.history.last().cloned(),
            guard.error.clone(),
        )
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/* -------------------------------------------------------------------------- */
/*                                 processes                                  */
/* -------------------------------------------------------------------------- */

struct ProcessCache {
    at: Instant,
    items: Vec<ProcessInfo>,
}

static PROCESS_CACHE: Mutex<Option<ProcessCache>> = Mutex::new(None);

pub fn list_processes(limit: usize) -> Vec<ProcessInfo> {
    {
        let guard = PROCESS_CACHE.lock().unwrap();
        if let Some(cache) = guard.as_ref() {
            if cache.at.elapsed() < Duration::from_secs(4) {
                return cache.items.iter().take(limit).cloned().collect();
            }
        }
    }

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);

    let mut items: Vec<ProcessInfo> = system
        .processes()
        .values()
        .filter(|process| process.memory() > 0)
        .map(|process| ProcessInfo {
            pid: process.pid().as_u32(),
            name: {
                let raw = process.name().to_string_lossy().to_string();
                if raw.to_lowercase().ends_with(".exe") {
                    raw[..raw.len() - 4].to_string()
                } else {
                    raw
                }
            },
            memory: process.memory(),
            cpu_percent: (process.cpu_usage() as f64 * 10.0).round() / 10.0,
            path: process.exe().map(|path| path.to_string_lossy().to_string()),
        })
        .collect();

    items.sort_by_key(|item| std::cmp::Reverse(item.memory));
    items.truncate(40);

    *PROCESS_CACHE.lock().unwrap() = Some(ProcessCache {
        at: Instant::now(),
        items: items.clone(),
    });

    items.into_iter().take(limit).collect()
}

pub fn kill_process(pid: u32) -> (bool, Option<String>) {
    if pid <= 4 {
        return (false, Some("Недопустимый идентификатор процесса".into()));
    }
    if pid == std::process::id() {
        return (false, Some("Нельзя завершить собственный процесс".into()));
    }
    if !list_processes(40).iter().any(|item| item.pid == pid) {
        return (false, Some("Процесс отсутствует в списке активных".into()));
    }

    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let target = Pid::from_u32(pid);

    match system.process(target) {
        Some(process) => {
            if process.kill() {
                *PROCESS_CACHE.lock().unwrap() = None;
                (true, None)
            } else {
                (
                    false,
                    Some("Не удалось завершить процесс (нужны права)".into()),
                )
            }
        }
        None => (false, Some("Процесс уже завершён".into())),
    }
}

pub fn state(processes: Vec<ProcessInfo>) -> MetricsState {
    let handle = global();
    let (mut history, latest, error) = handle.snapshot();

    // Per-core values are only needed for the latest sample.
    let latest_cores = latest
        .as_ref()
        .map(|sample| sample.cores.clone())
        .unwrap_or_default();
    for sample in history.iter_mut() {
        sample.cores.clear();
    }
    if let Some(last) = history.last_mut() {
        last.cores = latest_cores;
    }

    MetricsState {
        history,
        latest,
        interval_ms: SAMPLE_INTERVAL_MS,
        error,
        processes,
    }
}

static GLOBAL: Mutex<Option<Arc<MetricsHandle>>> = Mutex::new(None);

pub fn global() -> Arc<MetricsHandle> {
    let mut guard = GLOBAL.lock().unwrap();
    if guard.is_none() {
        *guard = Some(Arc::new(MetricsHandle::start()));
    }
    Arc::clone(guard.as_ref().unwrap())
}

pub fn stop_global() {
    if let Some(handle) = GLOBAL.lock().unwrap().take() {
        handle.stop();
    }
}
