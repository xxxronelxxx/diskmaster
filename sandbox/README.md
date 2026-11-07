# Sandbox testing

Windows Sandbox harness for Диск-Мастер. Test-only: the product itself contains
no PowerShell scripts and does not depend on them.

## How it works

`*.wsb` maps `D:\work\cleaner\.sandbox\share` into the sandbox as
`C:\Users\WDAGUtilityAccount\Desktop\share` (read-write) and runs a scenario at
logon. The host reads `result.json` / screenshots from the share and kills the
VM. The sandbox is disposable: deletions, DISM, restore points and registry
changes never touch the host.

`.sandbox/` is gitignored; it is the only folder shared with the sandbox. The
absolute `HostFolder` path in the `.wsb` files must be adjusted on another
machine.

## Scenarios

| File | What it does |
| --- | --- |
| `test.wsb` + `smoke.ps1` | Environment, WebView2, portable launch, screenshot, helper list |
| `test-destructive.wsb` + `destructive.ps1` | Fake `Windows.old`, restore points, system categories, DISM |
| `test-ui.wsb` + `ui.ps1` | Opens the maintenance tab via UI Automation |
| `test-ui2.wsb` + `ui2.ps1` | Creates a restore point through the modal and loads the list |
| `test-ui3.wsb` + `ui3.ps1` | Settings, notification test, process priority/suspend, dry run, global hotkeys |
| `test-hotkey.wsb` + `hotkey.ps1` | Minimizes the window and checks `Ctrl+Alt+D` / `Ctrl+Alt+W` |
| `test-install.wsb` + `install-test.ps1` | Silent NSIS install, helper placement, Start Menu shortcut, toast |
| `test-ui4.wsb` + `ui4.ps1` | Installs the app, presses «Установить и включить», installs PawnIO, checks the sensor service and stop |
| `test-ui5.wsb` + `ui5.ps1` | Settings: Run key → highest scheduled task → back, checks task XML |
| `test-ui-drivers.wsb` + `ui-drivers.ps1` | Drivers tab: renders list/empty state and action buttons |
| `test-uninstall.wsb` + `uninstall-test.ps1` | Creates tasks/Run keys, starts app + sensors, uninstalls, verifies cleanup |
| `test-upgrade.wsb` + `upgrade-test.ps1` | Installs 1.7.3, starts app + sensors, upgrades to 1.7.4 in place |

## Run

```bash
mkdir -p .sandbox/share/app
cp -r output/DiskMaster-portable-<ver>/* .sandbox/share/app/
cp sandbox/<scenario>.ps1 sandbox/run-<scenario>.cmd .sandbox/share/
powershell -NoProfile -Command "Start-Process 'C:\Windows\System32\WindowsSandbox.exe' -ArgumentList 'D:\work\cleaner\sandbox\test-<scenario>.wsb'"
```

Results appear in `.sandbox/share`; the progress log shows the last completed
phase if a run hangs. Scripts must stay UTF-8 with BOM: Windows PowerShell 5.1
otherwise reads Cyrillic as ANSI.

## Notes

- Windows Sandbox runs with `EnableLUA=0`, so the whole session is elevated and
  the UAC consent prompt itself cannot be tested there. The helper operations and
  the `runas` bridge are still exercised.
- Writing to the mapped folder is slow for some metadata operations; the scripts
  use local `C:\Temp` files for helper I/O and copy only the final text.
- Window navigation uses `--tab=<id>` (`--settings` also works): UI Automation
  clicks on the sidebar are unreliable because the list scrolls.
- Portable builds have no Start Menu shortcut, so Windows may swallow toasts.
  The installed build shows them (verified in `install-test.ps1`).
