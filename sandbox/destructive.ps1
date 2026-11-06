# Destructive tests for Диск-Мастер inside a disposable Windows Sandbox.
# Every target below is a fixture created in this VM; nothing on the host is touched.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$Helper = Join-Path $Share 'app\resources\DiskMasterHelper.exe'
$Temp = 'C:\Temp'
New-Item -ItemType Directory -Path $Temp -Force | Out-Null

$Result = [ordered]@{
  phase     = 'init'
  startedAt = (Get-Date).ToString('o')
  notes     = @()
  env       = [ordered]@{}
  fixtures  = [ordered]@{}
  ops       = [ordered]@{}
  checks    = [ordered]@{}
}

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  $json = $Result | ConvertTo-Json -Depth 10
  [IO.File]::WriteAllText((Join-Path $Share 'destructive.json'), $json, (New-Object System.Text.UTF8Encoding($false)))
  Add-Content -Path (Join-Path $Share 'destructive-progress.log') -Value ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Result.phase)
}

function Step([string]$Name) {
  $Result.phase = $Name
  Save-Result
}

function Invoke-Helper([string]$Op, $Payload, [string]$Tag) {
  $in = Join-Path $Temp "in-$Tag.json"
  $out = Join-Path $Temp "out-$Tag.json"
  Remove-Item $in, $out -Force -ErrorAction SilentlyContinue
  if ($null -eq $Payload) { $Payload = @{} }
  $body = @{ payload = $Payload } | ConvertTo-Json -Depth 6 -Compress
  [IO.File]::WriteAllText($in, $body, (New-Object System.Text.UTF8Encoding($false)))
  $sw = [Diagnostics.Stopwatch]::StartNew()
  & $Helper "--op=$Op" "--input=$in" "--output=$out"
  $code = $LASTEXITCODE
  $sw.Stop()
  $text = if (Test-Path $out) { [IO.File]::ReadAllText($out) } else { $null }
  $Result.ops[$Tag] = [ordered]@{ op = $Op; exit = $code; ms = $sw.ElapsedMilliseconds; output = $text }
  Save-Result
  return [ordered]@{ exit = $code; ms = $sw.ElapsedMilliseconds; text = $text }
}

function New-Fixture([string]$Path, [int]$Bytes) {
  $dir = Split-Path $Path -Parent
  New-Item -ItemType Directory -Path $dir -Force | Out-Null
  $buffer = New-Object byte[] $Bytes
  (New-Object Random 42).NextBytes($buffer)
  [IO.File]::WriteAllBytes($Path, $buffer)
}

try {
  Step 'env'
  $Result.env.computer = $env:COMPUTERNAME
  $Result.env.os = (Get-CimInstance Win32_OperatingSystem).Caption
  $Result.env.helperExists = Test-Path $Helper
  $Result.env.helperVersion = if (Test-Path $Helper) { (Get-Item $Helper).VersionInfo.FileVersion } else { $null }
  $Result.env.isAdmin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  Step 'env-done'

  # ---------- Windows.old ----------
  $oldRoot = 'C:\Windows.old'
  if (Test-Path $oldRoot) { Remove-Item $oldRoot -Recurse -Force -ErrorAction SilentlyContinue }
  New-Fixture "$oldRoot\Windows\System32\dm-1.dll" 524288
  New-Fixture "$oldRoot\Windows\System32\dm-2.dll" 524288
  New-Fixture "$oldRoot\Users\Test\file.dat" 1048576
  New-Fixture "$oldRoot\Users\Test\docs\file.txt" 65536
  $Result.fixtures.windowsOldFiles = @(Get-ChildItem $oldRoot -Recurse -File).Count
  $Result.fixtures.windowsOldBytes = (Get-ChildItem $oldRoot -Recurse -File | Measure-Object Length -Sum).Sum
  Step 'windows-old-fixture'

  $Result.ops.windowsOld = (Invoke-Helper 'windows_old' @{} 'windows-old')
  $Result.checks.windowsOldRemoved = -not (Test-Path $oldRoot)
  Step 'windows-old-done'

  # ---------- Restore points ----------
  $sr = [ordered]@{}
  try {
    Enable-ComputerRestore -Drive 'C:\' -ErrorAction Stop
    $sr.protectionEnabled = $true
  } catch {
    $sr.protectionEnabled = $false
    $sr.enableError = $_.Exception.Message
  }
  $sr.vss = (Get-Service VSS -ErrorAction SilentlyContinue).Status
  $Result.checks.restorePointSetup = $sr
  Step 'restore-setup'

  $create = Invoke-Helper 'restore_point_create' @{} 'sr-create'
  Start-Sleep -Seconds 5
  $list = Invoke-Helper 'restore_points_list' @{} 'sr-list'
  $Result.checks.restorePointCreateOk = $false
  $Result.checks.restorePointListCount = $null
  try {
    $createJson = ConvertFrom-Json $create.text
    $Result.checks.restorePointCreateOk = [bool]$createJson.ok
    $Result.checks.restorePointSequence = $createJson.data.sequence
  } catch {
    $Result.notes += "create parse: $($_.Exception.Message)"
  }
  try {
    $listJson = ConvertFrom-Json $list.text
    $Result.checks.restorePointListCount = @($listJson.data).Count
    $Result.checks.restorePointList = @($listJson.data | ForEach-Object { "$($_.sequence):$($_.description)" })
  } catch {
    $Result.notes += "list parse: $($_.Exception.Message)"
  }
  Step 'restore-create'

  if ($Result.checks.restorePointSequence) {
    $Result.ops.restoreDelete = (Invoke-Helper 'restore_point_delete' @{ sequence = $Result.checks.restorePointSequence } 'sr-delete')
    Start-Sleep -Seconds 3
    $listAfter = Invoke-Helper 'restore_points_list' @{} 'sr-list-after'
    try {
      $Result.checks.restorePointCountAfterDelete = @((ConvertFrom-Json $listAfter.text).data).Count
    } catch {
      $Result.notes += "list-after parse: $($_.Exception.Message)"
    }
  }
  Step 'restore-delete'

  # ---------- System cleanup categories ----------
  New-Fixture 'C:\Windows\Temp\dm-temp-a.bin' 1048576
  New-Fixture 'C:\Windows\Temp\dm-sub\dm-temp-b.bin' 524288
  New-Fixture 'C:\Windows\Prefetch\DM-TEST.pf' 65536
  New-Fixture 'C:\Windows\Logs\dm-test\dm-test.log' 131072
  New-Fixture 'C:\Windows\SoftwareDistribution\Download\dm-test\dm-update.bin' 2097152
  $Result.fixtures.tempSample = Test-Path 'C:\Windows\Temp\dm-temp-a.bin'
  $Result.fixtures.prefetchSample = Test-Path 'C:\Windows\Prefetch\DM-TEST.pf'
  $Result.fixtures.logsSample = Test-Path 'C:\Windows\Logs\dm-test\dm-test.log'
  $Result.fixtures.updateSample = Test-Path 'C:\Windows\SoftwareDistribution\Download\dm-test\dm-update.bin'
  Step 'clean-fixtures'

  $clean = Invoke-Helper 'clean_system' @{ categories = @('win-temp', 'win-update', 'prefetch', 'win-logs') } 'clean-system'
  $Result.checks.cleanExit = $clean.exit
  try {
    $cleanJson = ConvertFrom-Json $clean.text
    $Result.checks.cleanOk = [bool]$cleanJson.ok
    $Result.checks.cleanCategories = @($cleanJson.data | ForEach-Object { "$($_.id): files=$($_.files) bytes=$($_.bytes) errors=$(@($_.errors).Count)" })
  } catch {
    $Result.notes += "clean parse: $($_.Exception.Message)"
  }
  Start-Sleep -Seconds 2
  $Result.checks.tempSampleGone = -not (Test-Path 'C:\Windows\Temp\dm-temp-a.bin')
  $Result.checks.tempSubGone = -not (Test-Path 'C:\Windows\Temp\dm-sub')
  $Result.checks.prefetchSampleGone = -not (Test-Path 'C:\Windows\Prefetch\DM-TEST.pf')
  $Result.checks.logsSampleGone = -not (Test-Path 'C:\Windows\Logs\dm-test')
  $Result.checks.updateSampleGone = -not (Test-Path 'C:\Windows\SoftwareDistribution\Download\dm-test')
  Step 'clean-done'

  Step 'dism-start'
  $dism = Invoke-Helper 'dism_cleanup' @{} 'dism'
  $Result.checks.dismExit = $dism.exit
  try {
    $dismJson = ConvertFrom-Json $dism.text
    $Result.checks.dismOk = [bool]$dismJson.ok
    $Result.checks.dismTail = $dismJson.data.output
  } catch {
    $Result.notes += "dism parse: $($_.Exception.Message)"
  }
  Step 'dism-done'

  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
