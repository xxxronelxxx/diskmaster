use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use wait_timeout::ChildExt;

use crate::state::{AppState, UpdateCache};
use crate::types::{WingetListing, WingetUpdate};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const CACHE_TTL_MS: u128 = 120_000;

struct RunResult {
    output: String,
    code: Option<i32>,
}

fn run_winget(args: &[&str], timeout: Duration) -> RunResult {
    let command = format!("chcp 65001>nul && winget {}", args.join(" "));
    let mut child = match Command::new("cmd.exe")
        .args(["/c", &command])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return RunResult {
                output: error.to_string(),
                code: None,
            }
        }
    };

    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let reader = thread::spawn(move || {
        let mut output = String::new();
        if let Some(stream) = stdout.as_mut() {
            let _ = stream.read_to_string(&mut output);
        }
        if let Some(stream) = stderr.as_mut() {
            let _ = stream.read_to_string(&mut output);
        }
        output
    });

    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => Some(status),
        Ok(None) => {
            let _ = child.kill();
            child.wait().ok()
        }
        Err(_) => None,
    };

    RunResult {
        output: reader.join().unwrap_or_default(),
        code: status.and_then(|status| status.code()),
    }
}

fn parse_table(output: &str) -> Vec<WingetUpdate> {
    let mut rows = Vec::new();

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.chars().all(|c| c == '-' || c == '–' || c == '—' || c.is_whitespace()) {
            continue;
        }
        let lower = line.to_lowercase();
        if (lower.contains("пакет") || lower.contains("package")) && lower.contains("верси") {
            continue;
        }
        if lower.starts_with("name ") || lower.starts_with("имя ") {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 5 {
            continue;
        }
        let source = tokens[tokens.len() - 1];
        let available = tokens[tokens.len() - 2];
        let current = tokens[tokens.len() - 3];
        let id = tokens[tokens.len() - 4];
        let name = tokens[..tokens.len() - 4].join(" ");

        if !id.contains('.')
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
        {
            continue;
        }
        let available_ok = available.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
            || available.eq_ignore_ascii_case("unknown")
            || available.eq_ignore_ascii_case("неизвестно")
            || available == "—"
            || available == "-";
        if !available_ok {
            continue;
        }
        if !source
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
        {
            continue;
        }

        rows.push(WingetUpdate {
            name: if name.is_empty() { id.to_string() } else { name },
            id: id.to_string(),
            current: current.to_string(),
            available: available.to_string(),
            source: source.to_string(),
        });
    }

    rows
}

pub fn list(shared: &AppState, force: bool) -> WingetListing {
    if !force {
        let guard = shared.updates.lock().unwrap();
        if let Some(cache) = guard.as_ref() {
            if cache.at.elapsed().as_millis() < CACHE_TTL_MS {
                return cache.listing.clone();
            }
        }
    }

    let mut listing = WingetListing {
        available: false,
        updates: Vec::new(),
        error: None,
        raw: None,
        refreshed_at: chrono::Utc::now().timestamp_millis(),
    };

    let version = run_winget(&["--version"], Duration::from_secs(25));
    if version.output.chars().any(|c| c.is_ascii_digit()) && version.output.contains('.') {
        listing.available = true;
    } else {
        listing.error =
            Some("winget не найден. Установите «Установщик приложений» из Microsoft Store.".into());
        store(shared, &listing);
        return listing;
    }

    let result = run_winget(
        &[
            "upgrade",
            "--include-unknown",
            "--accept-source-agreements",
            "--disable-interactivity",
        ],
        Duration::from_secs(240),
    );

    listing.updates = parse_table(&result.output);
    listing.raw = Some(result.output.trim().chars().take(4000).collect());

    let lower = result.output.to_lowercase();
    if listing.updates.is_empty() && (lower.contains("error") || lower.contains("failed") || lower.contains("не удалось")) {
        listing.error = Some("Не удалось получить список обновлений winget".into());
    }

    store(shared, &listing);
    listing
}

fn store(shared: &AppState, listing: &WingetListing) {
    *shared.updates.lock().unwrap() = Some(UpdateCache {
        at: Instant::now(),
        listing: listing.clone(),
    });
}

pub fn upgrade(shared: &AppState, id: &str) -> (bool, String) {
    let listing = list(shared, false);
    if !listing.updates.iter().any(|update| update.id == id) {
        return (false, "Пакет отсутствует в списке доступных обновлений".into());
    }

    let result = run_winget(
        &[
            "upgrade",
            "--id",
            id,
            "--exact",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ],
        Duration::from_secs(600),
    );

    *shared.updates.lock().unwrap() = None;
    let lower = result.output.to_lowercase();
    let ok = result.code == Some(0)
        && !lower.contains("0x8a150")
        && !lower.contains("failed")
        && !lower.contains("error:");
    let tail: String = result
        .output
        .trim()
        .chars()
        .rev()
        .take(4000)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    (ok, tail)
}

pub fn upgrade_all(shared: &AppState) -> (bool, String) {
    let result = run_winget(
        &[
            "upgrade",
            "--all",
            "--include-unknown",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ],
        Duration::from_secs(1800),
    );

    *shared.updates.lock().unwrap() = None;
    let lower = result.output.to_lowercase();
    let ok = result.code == Some(0)
        && !lower.contains("0x8a150")
        && !lower.contains("failed")
        && !lower.contains("error:");
    let tail: String = result
        .output
        .trim()
        .chars()
        .rev()
        .take(4000)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    (ok, tail)
}

#[cfg(test)]
mod tests {
    use super::parse_table;

    #[test]
    fn parses_localized_winget_table() {
        let output = "Name                 Id             Version  Available Source\n\
                      ----------------------------------------------------------------\n\
                      7-Zip 26.04 (x64)    7zip.7zip      26.04.0  26.05     winget\n\
                      Git                  Git.Git        2.55.0   2.56.0    winget\n";
        let rows = parse_table(output);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "7zip.7zip");
        assert_eq!(rows[0].name, "7-Zip 26.04 (x64)");
        assert_eq!(rows[0].available, "26.05");
        assert_eq!(rows[1].current, "2.55.0");
    }
}
