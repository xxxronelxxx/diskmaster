# Verifies that the NSIS uninstaller removes tasks, Run keys and stops processes.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$Result = [ordered]@{ phase = 'init'; notes = @(); steps = @(); before = [ordered]@{}; after = [ordered]@{} }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  [IO.File]::WriteAllText((Join-Path $Share 'uninstall.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'uninstall-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
function Run-Exists([string]$Name) {
  $value = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name $Name -ErrorAction SilentlyContinue
  return [bool]$value
}
function Task-Exists([string]$Name) {
  schtasks /Query /TN $Name *> $null
  return ($LASTEXITCODE -eq 0)
}
function Snapshot {
  return [ordered]@{
    app = @(Get-Process -Name DiskMaster -ErrorAction SilentlyContinue).Count
    sensors = @(Get-Process -Name DiskMasterSensors -ErrorAction SilentlyContinue).Count
    autostartTask = Task-Exists 'DiskMasterAutostart'
    cleanTask = Task-Exists 'DiskMaster\DiskMasterAutoClean'
    runApp = Run-Exists 'DiskMaster'
    runWidget = Run-Exists 'DiskMasterWidget'
  }
}

try {
  Step 'install'
  $setup = Join-Path $Share 'DiskMaster-setup.exe'
  $process = Start-Process -FilePath $setup -ArgumentList '/S' -PassThru
  $process.WaitForExit(180000) | Out-Null
  $Result.installExit = $process.ExitCode
  Start-Sleep -Seconds 6

  Step 'locate'
  $exe = Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter 'DiskMaster.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $exe) { throw 'installed executable not found' }
  $dir = $exe.DirectoryName
  $Result.installDir = $dir
  $Result.version = $exe.VersionInfo.FileVersion

  Step 'prepare-state'
  $command = '"' + $exe.FullName + '" --hidden'
  $widget = '"' + $exe.FullName + '" --widget'
  Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'DiskMaster' -Value $command
  Set-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'DiskMasterWidget' -Value $widget
  schtasks /Create /TN DiskMasterAutostart /TR $command /SC ONLOGON /RL HIGHEST /F *> $null
  schtasks /Create /TN 'DiskMaster\DiskMasterAutoClean' /TR $command /SC DAILY /ST 03:00 /F *> $null

  Start-Process -FilePath $exe.FullName -ArgumentList '--hidden' | Out-Null
  $sensorExe = Join-Path $dir 'resources\sensors\DiskMasterSensors.exe'
  if (Test-Path $sensorExe) {
    Start-Process -FilePath $sensorExe -ArgumentList '--serve', '--out', "$env:TEMP\u-sensors.json", '--stop', "$env:TEMP\u-stop", '--parent', "$PID" | Out-Null
  } else {
    $Result.notes += 'sensor module missing'
  }
  Start-Sleep -Seconds 4
  $Result.before = Snapshot

  Step 'uninstall'
  $uninstaller = Join-Path $dir 'uninstall.exe'
  $Result.uninstallerExists = Test-Path $uninstaller
  if (-not $Result.uninstallerExists) { throw 'uninstaller not found' }
  $uninstall = Start-Process -FilePath $uninstaller -ArgumentList '/S' -PassThru
  $uninstall.WaitForExit(180000) | Out-Null
  $Result.uninstallExit = $uninstall.ExitCode
  Start-Sleep -Seconds 8

  Step 'verify'
  $Result.after = Snapshot
  $Result.dirExists = Test-Path $dir
  $Result.steps += [ordered]@{
    step = 'verify'
    appGone = ($Result.after.app -eq 0)
    sensorsGone = ($Result.after.sensors -eq 0)
    autostartTaskGone = (-not $Result.after.autostartTask)
    cleanTaskGone = (-not $Result.after.cleanTask)
    runAppGone = (-not $Result.after.runApp)
    runWidgetGone = (-not $Result.after.runWidget)
    installDirGone = (-not $Result.dirExists)
  }

  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
