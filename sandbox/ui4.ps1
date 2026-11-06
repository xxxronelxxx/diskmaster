# Installs 1.7.1 in the sandbox and exercises the sensor module:
# install PawnIO from the bundled signed installer, start the service, verify JSON.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$Result = [ordered]@{ phase = 'init'; notes = @(); steps = @(); install = [ordered]@{}; sensors = $null }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  [IO.File]::WriteAllText((Join-Path $Share 'ui4.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'ui4-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
function Save-Shot([string]$Name) {
  try {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $bmp.Save((Join-Path $Share $Name), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
  } catch { $Result.notes += "shot ${Name}: $($_.Exception.Message)" }
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$Root = [System.Windows.Automation.AutomationElement]::RootElement
$WindowCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Tauri Window')

function Get-AppWindow {
  $wins = @($Root.FindAll([System.Windows.Automation.TreeScope]::Children, $WindowCond))
  return $wins | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1
}
function Find-Named([string]$Name) {
  $win = Get-AppWindow
  if (-not $win) { return $null }
  $els = @($win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition))
  return $els | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1
}
function Find-Like([string]$Pattern) {
  $win = Get-AppWindow
  if (-not $win) { return $null }
  $els = @($win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition))
  return $els | Where-Object { $_.Current.Name -like $Pattern } | Select-Object -First 1
}
function Invoke-Element($el) {
  if (-not $el) { return 'missing' }
  $pattern = $null
  if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    return 'invoke'
  }
  return 'no-invoke'
}

try {
  Step 'install'
  $setup = Join-Path $Share 'DiskMaster-setup.exe'
  $process = Start-Process -FilePath $setup -ArgumentList '/S' -PassThru
  $process.WaitForExit(180000) | Out-Null
  $Result.install.exitCode = $process.ExitCode
  Start-Sleep -Seconds 6

  Step 'locate'
  $exe = Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter 'DiskMaster.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
  $Result.install.path = if ($exe) { $exe.FullName } else { $null }
  $Result.install.version = if ($exe) { $exe.VersionInfo.FileVersion } else { $null }
  $sensorsDir = if ($exe) { Join-Path $exe.DirectoryName 'resources\sensors' } else { $null }
  $Result.install.sensorsModule = if ($sensorsDir) { Test-Path (Join-Path $sensorsDir 'DiskMasterSensors.exe') } else { $false }
  $Result.install.pawnioSetup = if ($sensorsDir) { Test-Path (Join-Path $sensorsDir 'PawnIO_setup.exe') } else { $false }
  $Result.install.helper = if ($exe) { Test-Path (Join-Path $exe.DirectoryName 'resources\DiskMasterHelper.exe') } else { $false }
  if (-not $exe) { throw 'installed executable not found' }

  Step 'launch'
  Start-Process -FilePath $exe.FullName -ArgumentList '--tab=maintenance' | Out-Null
  Start-Sleep -Seconds 16
  $null = Get-AppWindow
  Save-Shot 'u4-off.png'
  $Result.steps += [ordered]@{ step = 'card'; installButton = [bool](Find-Named 'Установить и включить'); startButton = [bool](Find-Named 'Включить датчики') }

  Step 'open-install'
  $installButton = Find-Named 'Установить и включить'
  $how = Invoke-Element $installButton
  $Result.steps += [ordered]@{ step = 'open-install'; found = [bool]$installButton; how = $how }
  Start-Sleep -Seconds 2
  Save-Shot 'u4-modal.png'

  Step 'confirm-install'
  $confirm = Find-Named 'Установить'
  $how = Invoke-Element $confirm
  $Result.steps += [ordered]@{ step = 'confirm-install'; found = [bool]$confirm; how = $how }

  Step 'wait-install'
  $data = Join-Path $env:LOCALAPPDATA 'DiskMasterData\sensors'
  $json = Join-Path $data 'sensors.json'
  $deadline = (Get-Date).AddSeconds(180)
  while ((Get-Date) -lt $deadline -and -not (Test-Path $json)) { Start-Sleep -Seconds 2 }
  $Result.steps += [ordered]@{ step = 'snapshot'; exists = (Test-Path $json) }

  $installResult = Join-Path $data 'install-result.json'
  if (Test-Path $installResult) { $Result.install.result = [IO.File]::ReadAllText($installResult) }
  if (Test-Path $json) { $Result.sensors = [IO.File]::ReadAllText($json) }

  $pawnio = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO' -ErrorAction SilentlyContinue
  $Result.install.pawnioVersion = $pawnio.DisplayVersion
  $Result.install.service = @(Get-Process -Name DiskMasterSensors -ErrorAction SilentlyContinue).Count

  Start-Sleep -Seconds 2
  Save-Shot 'u4-on.png'
  $Result.steps += [ordered]@{ step = 'ui'; stopButton = [bool](Find-Named 'Остановить'); updated = [bool](Find-Like '*обновлено*') }

  Step 'stop'
  $stop = Find-Named 'Остановить'
  $how = Invoke-Element $stop
  Start-Sleep -Seconds 3
  $Result.steps += [ordered]@{ step = 'stop'; found = [bool]$stop; how = $how; service = @(Get-Process -Name DiskMasterSensors -ErrorAction SilentlyContinue).Count }

  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
