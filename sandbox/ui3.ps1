# UI test for 1.7.0: settings, notification test, process actions, dry run.
# Uses --tab=<id> so navigation does not depend on sidebar scroll position.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$App = Join-Path $Share 'app\DiskMaster.exe'
$Result = [ordered]@{ phase = 'init'; startedAt = (Get-Date).ToString('o'); notes = @(); steps = @() }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  [IO.File]::WriteAllText((Join-Path $Share 'ui3.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'ui3-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
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

Add-Type @'
using System;
using System.Runtime.InteropServices;
public class Win32Test {
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
}
'@
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$Root = [System.Windows.Automation.AutomationElement]::RootElement
$WindowCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Tauri Window')

function Get-AppWindow {
  $wins = @($Root.FindAll([System.Windows.Automation.TreeScope]::Children, $WindowCond))
  return $wins | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1
}
function Get-Elements($win) {
  if (-not $win) { return @() }
  return @($win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition))
}
function Find-Named([string]$Name) {
  $win = Get-AppWindow
  return Get-Elements $win | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1
}
function Find-Like([string]$Pattern) {
  $win = Get-AppWindow
  return Get-Elements $win | Where-Object { $_.Current.Name -like $Pattern } | Select-Object -First 1
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
function Start-App([string]$Tab) {
  Stop-Process -Name DiskMaster -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 3
  Start-Process -FilePath $App -ArgumentList "--tab=$Tab" | Out-Null
  Start-Sleep -Seconds 15
  $null = Get-Elements (Get-AppWindow)
  Start-Sleep -Seconds 1
}

try {
  Step 'prepare'
  New-Item -ItemType Directory -Path 'C:\Temp' -Force | Out-Null
  $data = Join-Path $Share 'app\disk-master-data'
  New-Item -ItemType Directory -Path $data -Force | Out-Null
  $settings = @{
    notifyLowDisk = $true
    lowDiskPercent = 25
    lowDiskCooldownMin = 15
    quickCleanNotify = $true
    minimizeToTray = $true
    protectPaths = @('D:\Protected')
    hotkeyShow = 'CmdOrCtrl+Alt+D'
    hotkeyWidget = 'CmdOrCtrl+Alt+W'
  } | ConvertTo-Json -Depth 4
  [IO.File]::WriteAllText((Join-Path $data 'settings.json'), $settings, (New-Object System.Text.UTF8Encoding($false)))

  New-Item -ItemType Directory -Path (Join-Path $env:TEMP 'dm-dry') -Force | Out-Null
  $bytes = New-Object byte[] (2MB)
  (New-Object Random 7).NextBytes($bytes)
  [IO.File]::WriteAllBytes((Join-Path $env:TEMP 'dm-dry\a.bin'), $bytes)
  [IO.File]::WriteAllBytes((Join-Path $env:TEMP 'dm-dry\b.bin'), (New-Object byte[] (1MB)))

  $source = @"
using System;
class P {
  static void Main() {
    var buffer = new byte[200 * 1024 * 1024];
    for (int i = 0; i < buffer.Length; i += 4096) buffer[i] = 1;
    System.Threading.Thread.Sleep(300000);
  }
}
"@
  [IO.File]::WriteAllText('C:\Temp\dmtest.cs', $source, (New-Object System.Text.UTF8Encoding($false)))
  $csc = "$env:SystemRoot\Microsoft.NET\Framework64\v4.0.30319\csc.exe"
  if (Test-Path $csc) { & $csc /nologo /out:C:\Temp\dmtest.exe C:\Temp\dmtest.cs | Out-Null }
  if (Test-Path 'C:\Temp\dmtest.exe') { Start-Process -FilePath 'C:\Temp\dmtest.exe' } else { $Result.notes += 'dmtest build failed' }
  Start-Sleep -Seconds 3

  # ------------------------------- settings -------------------------------
  Step 'settings'
  Start-App 'settings'
  Save-Shot 's1-settings.png'

  $threshold = Find-Like '*Порог свободного места*'
  $Result.steps += [ordered]@{ step = 'threshold'; found = [bool]$threshold }
  $Result.steps += [ordered]@{ step = 'protected-path'; found = [bool](Find-Like '*D:\Protected*') }

  $test = Find-Named 'Проверить'
  $how = Invoke-Element $test
  Start-Sleep -Seconds 4
  Save-Shot 's2-toast.png'
  $Result.steps += [ordered]@{ step = 'notification-test'; found = [bool]$test; how = $how }
  $Result.steps += [ordered]@{ step = 'notification-result'; text = if (Find-Like '*Уведомление отправлено*') { 'sent' } elseif (Find-Like '*Не удалось*') { 'failed' } else { 'unknown' } }

  $proc = Get-Process -Name DiskMaster -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
  if ($proc) {
    $handle = $proc.MainWindowHandle
    [Win32Test]::ShowWindow($handle, 0) | Out-Null
    Start-Sleep -Seconds 1
    $hiddenBefore = -not [Win32Test]::IsWindowVisible($handle)
    [Win32Test]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
    [Win32Test]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [Win32Test]::keybd_event(0x44, 0, 0, [UIntPtr]::Zero)
    [Win32Test]::keybd_event(0x44, 0, 2, [UIntPtr]::Zero)
    [Win32Test]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    [Win32Test]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Seconds 2
    $Result.steps += [ordered]@{ step = 'hotkey-show'; hiddenBefore = $hiddenBefore; visibleAfter = [Win32Test]::IsWindowVisible($handle) }
  } else {
    $Result.notes += 'no DiskMaster window for hotkey test'
  }

  # ------------------------------- monitor --------------------------------
  Step 'monitor'
  Start-App 'monitor'
  Start-Sleep -Seconds 5
  $manage = Find-Named 'Управление процессом dmtest'
  $Result.steps += [ordered]@{ step = 'manage-target'; found = [bool]$manage }
  $how = Invoke-Element $manage
  $Result.steps += [ordered]@{ step = 'open-manage'; how = $how }
  Start-Sleep -Seconds 2
  Save-Shot 's3-process-modal.png'

  $suspend = Find-Named 'Приостановить'
  $how = Invoke-Element $suspend
  $Result.steps += [ordered]@{ step = 'suspend'; found = [bool]$suspend; how = $how }
  Start-Sleep -Seconds 3
  Save-Shot 's4-suspended.png'
  $Result.steps += [ordered]@{ step = 'suspended-text'; found = [bool](Find-Like '*Процесс приостановлен*') }
  $Result.steps += [ordered]@{ step = 'resume-button'; found = [bool](Find-Named 'Продолжить') }

  $resume = Find-Named 'Продолжить'
  $how = Invoke-Element $resume
  $Result.steps += [ordered]@{ step = 'resume'; found = [bool]$resume; how = $how }
  Start-Sleep -Seconds 2

  $high = Find-Named 'Высокий'
  $how = Invoke-Element $high
  $Result.steps += [ordered]@{ step = 'priority-high'; found = [bool]$high; how = $how }
  Start-Sleep -Seconds 2
  Save-Shot 's5-priority.png'
  $Result.steps += [ordered]@{ step = 'priority-text'; found = [bool](Find-Like '*Приоритет изменён*') }
  $null = Invoke-Element (Find-Named 'Закрыть')

  # ------------------------------- cleanup --------------------------------
  Step 'cleanup'
  Start-App 'cleanup'
  $scan = Find-Named 'Начать проверку'
  $how = Invoke-Element $scan
  $Result.steps += [ordered]@{ step = 'start-scan'; found = [bool]$scan; how = $how }

  Step 'wait-scan'
  $dry = $null
  for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 2
    $dry = Find-Named 'Пробный запуск'
    if ($dry -and $dry.Current.IsEnabled) { break }
  }
  $Result.steps += [ordered]@{ step = 'dry-button'; found = [bool]$dry; enabled = if ($dry) { $dry.Current.IsEnabled } else { $false } }
  Save-Shot 's6-scan.png'

  Step 'dry-run'
  $how = Invoke-Element $dry
  $Result.steps += [ordered]@{ step = 'dry-run'; how = $how }
  Start-Sleep -Seconds 10
  Save-Shot 's7-dryrun.png'
  $Result.steps += [ordered]@{ step = 'dry-result'; found = [bool](Find-Like '*будет удалено*') }

  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
