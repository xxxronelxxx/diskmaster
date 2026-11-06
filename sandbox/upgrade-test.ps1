# Verifies an in-place upgrade: 1.7.3 -> 1.7.4, with the old app and sensor service running.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$Result = [ordered]@{ phase = 'init'; notes = @(); steps = @() }

function Save-Result { $Result.updatedAt=(Get-Date).ToString('o'); [IO.File]::WriteAllText((Join-Path $Share 'upgrade.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false))) }
function Step([string]$Name) { $Result.phase=$Name; Save-Result; Add-Content (Join-Path $Share 'upgrade-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
function Task-Exists([string]$Name) { schtasks /Query /TN $Name *> $null; return ($LASTEXITCODE -eq 0) }

try {
  Step 'install-old'
  $old = Join-Path $env:TEMP 'DiskMaster-1.7.3-setup.exe'
  Invoke-WebRequest -Uri 'https://github.com/xxxronelxxx/diskmaster/releases/download/v1.7.3/DiskMaster_1.7.3_x64-setup.exe' -OutFile $old -UseBasicParsing
  $p = Start-Process -FilePath $old -ArgumentList '/S' -PassThru
  $p.WaitForExit(180000) | Out-Null
  Start-Sleep 6

  Step 'locate-old'
  $exe = Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter 'DiskMaster.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $exe) { throw 'old exe not found' }
  $Result.oldVersion = $exe.VersionInfo.FileVersion
  $dir = $exe.DirectoryName

  Step 'running-state'
  $command = '"' + $exe.FullName + '" --hidden'
  schtasks /Create /TN DiskMasterAutostart /TR $command /SC ONLOGON /RL HIGHEST /F *> $null
  Start-Process -FilePath $exe.FullName -ArgumentList '--hidden' | Out-Null
  $sensorExe = Join-Path $dir 'resources\sensors\DiskMasterSensors.exe'
  if (Test-Path $sensorExe) {
    Start-Process -FilePath $sensorExe -ArgumentList '--serve', '--out', "$env:TEMP\up-sensors.json", '--stop', "$env:TEMP\up-stop", '--parent', "$PID" | Out-Null
  }
  Start-Sleep 4
  $Result.before = [ordered]@{
    app = @(Get-Process -Name DiskMaster -ErrorAction SilentlyContinue).Count
    sensors = @(Get-Process -Name DiskMasterSensors -ErrorAction SilentlyContinue).Count
    task = Task-Exists 'DiskMasterAutostart'
  }

  Step 'install-new'
  $new = Join-Path $Share 'DiskMaster-setup.exe'
  $p = Start-Process -FilePath $new -ArgumentList '/S' -PassThru
  $p.WaitForExit(180000) | Out-Null
  $Result.newExit = $p.ExitCode
  Start-Sleep 8

  Step 'verify'
  $exe = Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter 'DiskMaster.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
  $Result.newVersion = if ($exe) { $exe.VersionInfo.FileVersion } else { $null }
  $Result.after = [ordered]@{
    app = @(Get-Process -Name DiskMaster -ErrorAction SilentlyContinue).Count
    sensors = @(Get-Process -Name DiskMasterSensors -ErrorAction SilentlyContinue).Count
    task = Task-Exists 'DiskMasterAutostart'
  }
  $Result.steps += [ordered]@{
    step = 'verify'
    oldVersion = $Result.oldVersion
    newVersion = $Result.newVersion
    upgraded = ($Result.newVersion -eq '1.7.4')
    oldProcessesStopped = ($Result.after.app -eq 0 -and $Result.after.sensors -eq 0)
    taskKept = $Result.after.task
  }

  Step 'done'
} catch { $Result.notes += "fatal: $($_.Exception.Message)"; Step 'fatal' } finally { Save-Result; Step 'finished' }
