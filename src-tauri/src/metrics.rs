use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::ps;
use crate::types::{MetricsSample, MetricsState, ProcessInfo};

const HISTORY_SIZE: usize = 240;
pub const SAMPLE_INTERVAL_MS: u64 = 1000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSample {
    t: f64,
    cpu: f64,
    #[serde(default)]
    cores: Vec<f64>,
    ram_used: f64,
    ram_total: f64,
    disk_read: f64,
    disk_write: f64,
    net_in: f64,
    net_out: f64,
    net_adapter: Option<String>,
}

#[derive(Default)]
struct Store {
    history: Vec<MetricsSample>,
    error: Option<String>,
}

pub struct MetricsHandle {
    store: Arc<Mutex<Store>>,
    child: Arc<Mutex<Option<Child>>>,
    running: Arc<AtomicBool>,
}

impl MetricsHandle {
    pub fn start() -> Self {
        let handle = Self {
            store: Arc::new(Mutex::new(Store::default())),
            child: Arc::new(Mutex::new(None)),
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
        let child_slot = Arc::clone(&self.child);
        let running = Arc::clone(&self.running);
        let script = include_str!("../scripts/metrics.ps1");

        std::thread::spawn(move || {
            let mut restarts = 0;
            while running.load(Ordering::SeqCst) {
                let mut child = match ps::spawn_stream(script) {
                    Ok(child) => child,
                    Err(error) => {
                        store.lock().unwrap().error = Some(error.to_string());
                        break;
                    }
                };

                let stdout = child.stdout.take();
                *child_slot.lock().unwrap() = Some(child);

                if let Some(stdout) = stdout {
                    use std::io::BufRead;
                    let mut reader = std::io::BufReader::new(stdout);
                    let mut buffer = Vec::new();
                    loop {
                        buffer.clear();
                        match reader.read_until(b'\n', &mut buffer) {
                            Ok(0) => break,
                            Ok(_) => {}
                            Err(_) => break,
                        }
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }
                        let line = String::from_utf8_lossy(&buffer);
                        let trimmed = line.trim();
                        if !trimmed.starts_with('{') {
                            continue;
                        }
                        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                            continue;
                        };
                        if let Some(message) = value.get("error").and_then(|v| v.as_str()) {
                            store.lock().unwrap().error = Some(message.to_string());
                            continue;
                        }
                        let Ok(raw) = serde_json::from_value::<RawSample>(value) else {
                            continue;
                        };
                        restarts = 0;
                        let sample = MetricsSample {
                            t: raw.t as i64,
                            cpu: (raw.cpu * 10.0).round() / 10.0,
                            cores: raw.cores,
                            ram_used: raw.ram_used.max(0.0) as u64,
                            ram_total: raw.ram_total.max(0.0) as u64,
                            disk_read: raw.disk_read.max(0.0),
                            disk_write: raw.disk_write.max(0.0),
                            net_in: raw.net_in.max(0.0),
                            net_out: raw.net_out.max(0.0),
                            net_adapter: raw.net_adapter,
                        };
                        let mut guard = store.lock().unwrap();
                        guard.error = None;
                        guard.history.push(sample);
                        if guard.history.len() > HISTORY_SIZE {
                            let cut = guard.history.len() - HISTORY_SIZE;
                            guard.history.drain(0..cut);
                        }
                    }
                }

                *child_slot.lock().unwrap() = None;
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                restarts += 1;
                if restarts > 4 {
                    store.lock().unwrap().error =
                        Some("Сборщик метрик остановлен после нескольких сбоев".into());
                    running.store(false, Ordering::SeqCst);
                    break;
                }
                std::thread::sleep(Duration::from_secs(2));
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
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/* -------------------------------------------------------------------------- */
/*                                 processes                                  */
/* -------------------------------------------------------------------------- */

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProcess {
    pid: u32,
    name: String,
    memory: f64,
    cpu_seconds: f64,
    path: Option<String>,
}

static PROCESS_CACHE: Mutex<Option<(Instant, Vec<ProcessInfo>)>> = Mutex::new(None);

const PROCESS_SCRIPT: &str = concat!(
    "$items = Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.WorkingSet64 -gt 0 } | ",
    "Sort-Object WorkingSet64 -Descending | Select-Object -First 30 | ForEach-Object { ",
    "[pscustomobject]@{ pid = $_.Id; name = $_.ProcessName; memory = [double]$_.WorkingSet64; ",
    "cpuSeconds = if ($_.CPU) { [double]$_.CPU } else { 0 }; path = if ($_.Path) { [string]$_.Path } else { $null } } }; ",
    "ConvertTo-Json -InputObject @($items) -Depth 3 -Compress"
);

pub fn list_processes(limit: usize) -> Vec<ProcessInfo> {
    {
        let guard = PROCESS_CACHE.lock().unwrap();
        if let Some((at, items)) = guard.as_ref() {
            if at.elapsed() < Duration::from_secs(5) {
                return items.iter().take(limit).cloned().collect();
            }
        }
    }

    let data = ps::run_json::<Vec<RawProcess>>(PROCESS_SCRIPT, Duration::from_secs(25));
    let items: Vec<ProcessInfo> = data
        .unwrap_or_default()
        .into_iter()
        .filter(|item| !item.name.is_empty())
        .map(|item| ProcessInfo {
            pid: item.pid,
            name: item.name,
            memory: item.memory.max(0.0) as u64,
            cpu_seconds: item.cpu_seconds,
            path: item.path,
        })
        .collect();

    *PROCESS_CACHE.lock().unwrap() = Some((Instant::now(), items.clone()));
    items.into_iter().take(limit).collect()
}

pub fn kill_process(pid: u32) -> (bool, Option<String>) {
    if pid <= 4 {
        return (false, Some("Недопустимый идентификатор процесса".into()));
    }
    if pid == std::process::id() {
        return (false, Some("Нельзя завершить собственный процесс".into()));
    }
    let known = list_processes(40).iter().any(|item| item.pid == pid);
    let _ = known; // the list is advisory; the pid still has to exist

    let script = format!(
        "try {{ Stop-Process -Id {pid} -Force -ErrorAction Stop; \
         [pscustomobject]@{{ ok = $true }} | ConvertTo-Json -Compress }} catch {{ \
         [pscustomobject]@{{ ok = $false; error = [string]$_.Exception.Message }} | ConvertTo-Json -Compress }}"
    );
    let result = ps::run_json::<serde_json::Value>(&script, Duration::from_secs(20));
    *PROCESS_CACHE.lock().unwrap() = None;

    match result {
        Some(value) if value.get("ok").and_then(|v| v.as_bool()) == Some(true) => (true, None),
        Some(value) => (
            false,
            Some(
                value
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Не удалось завершить процесс")
                    .to_string(),
            ),
        ),
        None => (false, Some("Не удалось завершить процесс (нужны права)".into())),
    }
}

pub fn state(processes: Vec<ProcessInfo>) -> MetricsState {
    let handle = global();
    let (history, latest, error) = handle.snapshot();
    MetricsState {
        history,
        latest,
        interval_ms: SAMPLE_INTERVAL_MS,
        error: if cfg!(windows) { error } else { Some("Мониторинг доступен только в Windows".into()) },
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
