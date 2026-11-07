use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::guard;
use crate::paths::home_dir;
use crate::state::AppState;
use crate::system;
use crate::types::{DeleteResult, FolderAnalysis, FolderEntry};

const MAX_ENTRIES: usize = 500;

#[allow(dead_code)]
struct MeasureResult {
    size: u64,
    files: u64,
    truncated: bool,
}

fn measure(target: &Path, budget: Duration) -> MeasureResult {
    let stats = crate::walk::walk(target, Some(24), budget, None, |_| {}, |_, _, _| {});
    MeasureResult {
        size: stats.bytes,
        files: stats.files,
        truncated: stats.truncated,
    }
}

pub fn roots() -> Vec<serde_json::Value> {
    let home = home_dir();
    let mut list: Vec<(String, PathBuf)> = vec![
        ("Домашняя папка".into(), home.clone()),
        ("Загрузки".into(), home.join("Downloads")),
        ("Документы".into(), home.join("Documents")),
        ("Изображения".into(), home.join("Pictures")),
        ("Видео".into(), home.join("Videos")),
    ];

    for drive in system::drives() {
        list.push((
            if drive.label.is_empty() {
                drive.letter.clone()
            } else {
                format!("{} · {}", drive.letter, drive.label)
            },
            PathBuf::from(format!("{}\\", drive.letter)),
        ));
    }

    list.into_iter()
        .map(|(label, path)| {
            serde_json::json!({
                "label": label,
                "path": path.to_string_lossy(),
            })
        })
        .collect()
}

pub fn analyze(shared: &AppState, target: Option<String>) -> FolderAnalysis {
    let started = Instant::now();
    let root = target
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(home_dir);

    let mut result = FolderAnalysis {
        path: root.to_string_lossy().to_string(),
        name: root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| root.to_string_lossy().to_string()),
        size: 0,
        files: 0,
        entries: Vec::new(),
        parent: root
            .parent()
            .filter(|parent| *parent != root)
            .map(|parent| parent.to_string_lossy().to_string()),
        elapsed_ms: 0,
        truncated: false,
        error: None,
    };

    let dirents = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) => {
            result.error = Some(if error.kind() == std::io::ErrorKind::PermissionDenied {
                "Нет доступа к папке — нужны права администратора".into()
            } else {
                "Не удалось прочитать папку".into()
            });
            result.elapsed_ms = started.elapsed().as_millis() as u64;
            return result;
        }
    };

    let children: Vec<PathBuf> = dirents
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            !path
                .symlink_metadata()
                .map(|meta| meta.file_type().is_symlink())
                .unwrap_or(false)
        })
        .collect();

    let total = children.len().max(1);
    let budget_per_entry =
        Duration::from_millis(9_000 / total as u64).max(Duration::from_millis(600));

    let entries: Vec<Option<FolderEntry>> = children
        .par_iter()
        .map(|path| {
            let metadata = path.metadata().ok()?;
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();

            if metadata.is_dir() {
                let measured = measure(path, budget_per_entry);
                let children_count = fs::read_dir(path)
                    .map(|entries| entries.flatten().count() as u64)
                    .unwrap_or(0);
                Some(FolderEntry {
                    path: path.to_string_lossy().to_string(),
                    name,
                    is_dir: true,
                    size: measured.size,
                    files: measured.files,
                    modified: chrono::Utc::now().timestamp_millis(),
                    children: children_count,
                })
            } else if metadata.is_file() {
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|value| value.as_millis() as i64)
                    .unwrap_or(0);
                Some(FolderEntry {
                    path: path.to_string_lossy().to_string(),
                    name,
                    is_dir: false,
                    size: metadata.len(),
                    files: 1,
                    modified,
                    children: 0,
                })
            } else {
                None
            }
        })
        .collect();

    let mut entries: Vec<FolderEntry> = entries.into_iter().flatten().collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.size));

    result.size = entries.iter().map(|entry| entry.size).sum();
    result.files = entries.iter().map(|entry| entry.files).sum();
    result.truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    result.entries = entries;
    result.elapsed_ms = started.elapsed().as_millis() as u64;
    if result.elapsed_ms > 1_000 {
        crate::logging::log(&format!(
            "folders: {} ms, entries={}, truncated={}",
            result.elapsed_ms,
            result.entries.len(),
            result.truncated
        ));
    }

    {
        let mut allow = shared.allow.lock().unwrap();
        for entry in &result.entries {
            allow.register(Path::new(&entry.path));
        }
    }

    result
}

pub fn delete(shared: &AppState, paths: &[String], permanent: bool) -> DeleteResult {
    let mut result = DeleteResult {
        ok: true,
        bytes: 0,
        files: 0,
        errors: Vec::new(),
    };

    for raw in paths {
        let path = PathBuf::from(raw);
        let resolved = {
            let allow = shared.allow.lock().unwrap();
            match guard::assert_deletable(&path, &allow) {
                Ok(path) => path,
                Err(error) => {
                    result.ok = false;
                    result.errors.push(error);
                    continue;
                }
            }
        };

        let metadata = match resolved.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                result.ok = false;
                result
                    .errors
                    .push(format!("Не найдено: {}", resolved.display()));
                continue;
            }
        };

        let size = if metadata.is_dir() {
            measure(&resolved, Duration::from_secs(30)).size
        } else {
            metadata.len()
        };
        let files = 1;

        if permanent {
            let removal = if metadata.is_dir() {
                fs::remove_dir_all(&resolved)
            } else {
                fs::remove_file(&resolved)
            };
            match removal {
                Ok(()) => {
                    result.bytes += size;
                    result.files += files;
                }
                Err(error) => {
                    result.ok = false;
                    result
                        .errors
                        .push(if error.kind() == std::io::ErrorKind::PermissionDenied {
                            format!("Нет прав: {}", resolved.display())
                        } else {
                            format!("Ошибка: {}", resolved.display())
                        });
                }
            }
        } else {
            match trash::delete(&resolved) {
                Ok(()) => {
                    result.bytes += size;
                    result.files += files;
                }
                Err(error) => {
                    result.ok = false;
                    result
                        .errors
                        .push(format!("{}: {error}", resolved.display()));
                }
            }
        }
    }

    result
}
