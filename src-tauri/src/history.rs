use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::paths::storage_path;
use crate::system;
use crate::types::{DriveSnapshot, HistoryCaptureResult, HistorySummary, SnapshotDrive};

const MAX_POINTS: usize = 2000;
const SAMPLE_EVERY_MS: i64 = 30 * 60 * 1000;

fn file() -> std::path::PathBuf {
    storage_path(&["history", "drives.json"])
}

pub fn read_points() -> Vec<DriveSnapshot> {
    let Ok(raw) = fs::read_to_string(file()) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<DriveSnapshot>>(&raw).unwrap_or_default()
}

fn write_points(points: &[DriveSnapshot]) {
    let path = file();
    crate::paths::ensure_parent(&path);
    let trimmed = if points.len() > MAX_POINTS {
        &points[points.len() - MAX_POINTS..]
    } else {
        points
    };
    if let Ok(raw) = serde_json::to_string(trimmed) {
        let _ = fs::write(path, raw);
    }
}

fn make_snapshot() -> Option<DriveSnapshot> {
    let drives = system::drives();
    if drives.is_empty() {
        return None;
    }
    Some(DriveSnapshot {
        t: chrono::Utc::now().timestamp_millis(),
        drives: drives
            .into_iter()
            .map(|drive| SnapshotDrive {
                letter: drive.letter,
                used: drive.used,
                free: drive.free,
                total: drive.total,
            })
            .collect(),
    })
}

fn same_values(a: &DriveSnapshot, b: &DriveSnapshot) -> bool {
    if a.drives.len() != b.drives.len() {
        return false;
    }
    a.drives.iter().zip(b.drives.iter()).all(|(left, right)| {
        left.letter == right.letter && left.used.abs_diff(right.used) < 20 * 1024 * 1024
    })
}

pub fn capture() -> Option<DriveSnapshot> {
    let snapshot = make_snapshot()?;
    let points = read_points();
    let now = chrono::Utc::now().timestamp_millis();

    if let Some(last) = points.last() {
        if same_values(last, &snapshot) && now - last.t < SAMPLE_EVERY_MS {
            return None;
        }
    }

    let mut next = points;
    next.push(snapshot.clone());
    write_points(&next);
    Some(snapshot)
}

pub fn summary() -> HistorySummary {
    let points = read_points();
    let mut letters: Vec<String> = points
        .iter()
        .flat_map(|point| point.drives.iter().map(|drive| drive.letter.clone()))
        .collect();
    letters.sort();
    letters.dedup();

    let mut growth_per_day = 0.0;
    if points.len() >= 2 {
        let first = &points[0];
        let last = &points[points.len() - 1];
        let days = (last.t - first.t) as f64 / 86_400_000.0;
        if days >= 0.2 {
            let first_used: u64 = first.drives.iter().map(|drive| drive.used).sum();
            let last_used: u64 = last.drives.iter().map(|drive| drive.used).sum();
            growth_per_day = (last_used as f64 - first_used as f64) / days;
        }
    }

    HistorySummary {
        first_at: points.first().map(|point| point.t),
        points,
        letters,
        growth_per_day,
    }
}

pub fn capture_with_summary() -> HistoryCaptureResult {
    let snapshot = capture();
    let summary = summary();
    HistoryCaptureResult {
        snapshot,
        points: summary.points,
        letters: summary.letters,
        growth_per_day: summary.growth_per_day,
        first_at: summary.first_at,
    }
}

static STARTED: AtomicBool = AtomicBool::new(false);

/// Samples free space every 30 minutes for as long as the app is running.
pub fn start_sampler() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        let _ = capture();
        loop {
            std::thread::sleep(Duration::from_millis(SAMPLE_EVERY_MS as u64));
            let _ = capture();
        }
    });
}
