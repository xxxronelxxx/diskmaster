//! Fast directory walker shared by cleanup, large-file search and folder sizes.
//!
//! `FindFirstFileExW` returns the size and timestamp together with the name, so
//! the caller does not need a separate `stat` for every file. This is the same
//! approach already used by the duplicate scanner.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::{
    FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, WIN32_FIND_DATAW,
};

const FIND_FIRST_EX_LARGE_FETCH: u32 = 0x0000_0002;

#[derive(Debug, Clone)]
pub struct WalkEntry {
    pub path: PathBuf,
    pub size: u64,
    pub modified: i64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WalkStats {
    pub files: u64,
    pub bytes: u64,
    pub dirs: u64,
    pub denied: u64,
    pub truncated: bool,
    pub canceled: bool,
}

fn wide_path(path: &Path) -> Vec<u16> {
    let text = path.to_string_lossy();
    let prefixed = if text.len() > 240 && !text.starts_with("\\\\?\\") {
        format!("\\\\?\\{text}")
    } else {
        text.to_string()
    };
    prefixed.encode_utf16().chain(std::iter::once(0)).collect()
}

fn filetime_to_millis(high: u32, low: u32) -> i64 {
    let ticks = ((high as u64) << 32 | low as u64) as i64;
    ((ticks - 116_444_736_000_000_000) / 10_000).max(0)
}

/// Walks `root` without following reparse points. `on_file` receives files only.
/// Progress is reported at most every 250 ms.
pub fn walk<F, P>(
    root: &Path,
    max_depth: Option<usize>,
    budget: Duration,
    cancel: Option<&AtomicBool>,
    mut on_file: F,
    mut on_progress: P,
) -> WalkStats
where
    F: FnMut(&WalkEntry),
    P: FnMut(u64, u64, &Path),
{
    let started = Instant::now();
    let mut stats = WalkStats::default();
    if !root.is_dir() {
        return stats;
    }

    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut last_report = Instant::now();

    while let Some((directory, depth)) = stack.pop() {
        if cancel
            .map(|flag| flag.load(Ordering::Relaxed))
            .unwrap_or(false)
        {
            stats.canceled = true;
            break;
        }
        if started.elapsed() > budget {
            stats.truncated = true;
            break;
        }

        let pattern = directory.join("*");
        let wide = wide_path(&pattern);
        let mut data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
        let handle = unsafe {
            FindFirstFileExW(
                wide.as_ptr(),
                FindExInfoBasic,
                &mut data as *mut _ as *mut c_void,
                FindExSearchNameMatch,
                std::ptr::null(),
                FIND_FIRST_EX_LARGE_FETCH,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            // An empty directory is not an access error.
            let code = std::io::Error::last_os_error().raw_os_error();
            if code != Some(2) && code != Some(18) {
                stats.denied += 1;
            }
            continue;
        }

        loop {
            if cancel
                .map(|flag| flag.load(Ordering::Relaxed))
                .unwrap_or(false)
            {
                stats.canceled = true;
                break;
            }
            if started.elapsed() > budget {
                stats.truncated = true;
                break;
            }

            let end = data
                .cFileName
                .iter()
                .position(|&value| value == 0)
                .unwrap_or(data.cFileName.len());
            let name = String::from_utf16_lossy(&data.cFileName[..end]);

            if name != "." && name != ".." {
                let attributes = data.dwFileAttributes;
                if attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
                    let path = directory.join(&name);
                    if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                        stats.dirs += 1;
                        let can_descend = max_depth.map(|max| depth < max).unwrap_or(true);
                        if can_descend {
                            stack.push((path, depth + 1));
                        }
                    } else {
                        let size = ((data.nFileSizeHigh as u64) << 32) | data.nFileSizeLow as u64;
                        let entry = WalkEntry {
                            path,
                            size,
                            modified: filetime_to_millis(
                                data.ftLastWriteTime.dwHighDateTime,
                                data.ftLastWriteTime.dwLowDateTime,
                            ),
                        };
                        stats.files += 1;
                        stats.bytes += size;
                        on_file(&entry);
                    }
                }
            }

            if last_report.elapsed() > Duration::from_millis(250) {
                last_report = Instant::now();
                on_progress(stats.files, stats.bytes, &directory);
            }

            let mut next: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
            if unsafe { FindNextFileW(handle, &mut next) } == 0 {
                break;
            }
            data = next;
        }

        unsafe {
            FindClose(handle);
        }
        if stats.canceled || stats.truncated {
            break;
        }
    }

    stats
}

#[cfg(test)]
mod tests {
    use super::walk;
    use std::time::Duration;

    #[test]
    fn counts_files_and_sizes() {
        let root = std::env::temp_dir().join(format!("dm-walk-{}", std::process::id()));
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("a.txt"), b"1234").unwrap();
        std::fs::write(sub.join("b.txt"), b"12").unwrap();

        let stats = walk(
            &root,
            None,
            Duration::from_secs(10),
            None,
            |_| {},
            |_, _, _| {},
        );
        assert_eq!(stats.files, 2);
        assert_eq!(stats.bytes, 6);
        assert!(!stats.truncated);
        assert!(!stats.canceled);

        let limited = walk(
            &root,
            Some(0),
            Duration::from_secs(10),
            None,
            |_| {},
            |_, _, _| {},
        );
        assert_eq!(limited.files, 1);

        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let empty_stats = walk(
            &empty,
            None,
            Duration::from_secs(10),
            None,
            |_| {},
            |_, _, _| {},
        );
        assert_eq!(empty_stats.files, 0);
        assert_eq!(empty_stats.denied, 0);

        let _ = std::fs::remove_dir_all(root);
    }
}
