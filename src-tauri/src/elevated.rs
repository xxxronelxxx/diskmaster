use std::fs;
use std::path::PathBuf;

use serde_json::Value;

/// Locates the helper next to the executable (dev) or in `resources` (installed
/// and portable).
pub fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidates = [
        dir.join("DiskMasterHelper.exe"),
        dir.join("resources").join("DiskMasterHelper.exe"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

/// Runs the helper with UAC elevation and returns its JSON result.
pub fn run(operation: &str, payload: Value) -> Result<Value, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};

    let helper = helper_path().ok_or_else(|| {
        "Helper не найден. Пересоберите приложение (pnpm build:portable / build:installer)"
            .to_string()
    })?;

    let stamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("disk-master-helper-{stamp}"));
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let input_path = dir.join("input.json");
    let output_path = dir.join("output.json");
    fs::write(
        &input_path,
        serde_json::to_string(&serde_json::json!({ "payload": payload }))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let parameters = format!(
        "--op={operation} --input=\"{}\" --output=\"{}\"",
        input_path.display(),
        output_path.display()
    );

    let wide = |value: &str| -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let verb = wide("runas");
    let file = wide(&helper.to_string_lossy());
    let args = wide(&parameters);
    let directory = wide(
        &helper
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
    );

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = 0x0000_0100 | 0x0000_0040; // NOASYNC | NOCLOSEPROCESS
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = args.as_ptr();
    info.lpDirectory = directory.as_ptr();
    info.nShow = 0;

    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let error = std::io::Error::last_os_error();
        let code = error.raw_os_error().unwrap_or(0);
        let _ = fs::remove_dir_all(&dir);
        if code == 1223 {
            return Err("Запрос прав администратора отменён".into());
        }
        return Err(format!("Не удалось запустить helper: {error}"));
    }

    let handle = info.hProcess;
    if handle.is_null() {
        let _ = fs::remove_dir_all(&dir);
        return Err("Helper не вернул процесс".into());
    }

    let wait = unsafe { WaitForSingleObject(handle, INFINITE) };
    let mut exit_code: u32 = 1;
    unsafe {
        GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
    }
    let _ = wait;

    let result = fs::read_to_string(&output_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let _ = fs::remove_dir_all(&dir);

    match result {
        Some(value) => Ok(value),
        None => Err(format!(
            "Helper завершился без результата (код {exit_code})"
        )),
    }
}
