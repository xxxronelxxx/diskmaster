$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$App = Join-Path $Share 'app\DiskMaster.exe'
$Result = [ordered]@{ phase = 'init'; notes = @(); steps = @(); log = @() }
function Save-Result { [IO.File]::WriteAllText((Join-Path $Share 'hotkey.json'), ($Result | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding($false))) }
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'hotkey-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
function Save-Shot([string]$Name) {
  Add-Type -AssemblyName System.Windows.Forms
  Add-Type -AssemblyName System.Drawing
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save((Join-Path $Share $Name), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class W32 {
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
}
'@
function Send-CtrlAltD {
  [W32]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x44, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x44, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
}
try {
  Step 'launch'
  Stop-Process -Name DiskMaster -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
  Start-Process -FilePath $App -ArgumentList '--tab=settings' | Out-Null
  Start-Sleep -Seconds 16
  $logPath = Join-Path $Share 'app\disk-master-data\logs\app.log'
  if (Test-Path $logPath) { $Result.log = @(Get-Content $logPath | Select-Object -Last 20) }
  Save-Shot 'h1-visible.png'

  Step 'hide'
  $proc = Get-Process -Name DiskMaster -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
  if (-not $proc) { throw 'no app window' }
  $handle = $proc.MainWindowHandle
  [W32]::ShowWindow($handle, 6) | Out-Null
  Start-Sleep -Seconds 2
  $Result.steps += [ordered]@{ step = 'minimized'; visible = [W32]::IsWindowVisible($handle); iconic = [W32]::IsIconic($handle) }
  Save-Shot 'h2-hidden.png'

  Step 'hotkey'
  Send-CtrlAltD
  Start-Sleep -Seconds 3
  $Result.steps += [ordered]@{ step = 'hotkey'; visible = [W32]::IsWindowVisible($handle); iconic = [W32]::IsIconic($handle) }
  Save-Shot 'h3-after-hotkey.png'
  if (Test-Path $logPath) { $Result.log = @(Get-Content $logPath | Select-Object -Last 20) }

  Step 'widget'
  [W32]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x57, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x57, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 80
  [W32]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
  Start-Sleep -Seconds 3
  Save-Shot 'h4-widget.png'
  $Result.steps += [ordered]@{ step = 'widget'; widget = [bool](Get-Process -Name DiskMaster -ErrorAction SilentlyContinue) }
  if (Test-Path $logPath) { $Result.log = @(Get-Content $logPath | Select-Object -Last 30) }
  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally { Save-Result; Step 'finished' }
