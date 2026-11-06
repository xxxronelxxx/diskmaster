# Settings test: autostart + always-admin task policy.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$Result = [ordered]@{ phase = 'init'; notes = @(); steps = @(); install = [ordered]@{} }
function Save-Result { $Result.updatedAt=(Get-Date).ToString('o'); [IO.File]::WriteAllText((Join-Path $Share 'ui5.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false))) }
function Step([string]$Name) { $Result.phase=$Name; Save-Result; Add-Content (Join-Path $Share 'ui5-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
function Save-Shot([string]$Name) {
  Add-Type -AssemblyName System.Windows.Forms,System.Drawing
  $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp=New-Object System.Drawing.Bitmap($b.Width,$b.Height)
  $g=[System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size)
  $bmp.Save((Join-Path $Share $Name),[System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}
Add-Type @'
using System;
using System.Runtime.InteropServices;
public class W5 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, int data, UIntPtr extra);
  public static void Wheel(int x, int y, int delta) { SetCursorPos(x, y); mouse_event(0x0800, 0, 0, delta, UIntPtr.Zero); }
}
'@
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$Root=[System.Windows.Automation.AutomationElement]::RootElement
$Cond=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty,'Tauri Window')
function Get-Win { $w=@($Root.FindAll([System.Windows.Automation.TreeScope]::Children,$Cond)); return $w | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1 }
function Get-Els { $w=Get-Win; if(-not $w){return @()}; return @($w.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)) }
function Find-Named([string]$Name) { return Get-Els | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1 }
function Find-Like([string]$Pattern) { return Get-Els | Where-Object { $_.Current.Name -like $Pattern } | Select-Object -First 1 }
function Find-Switch([string]$Name) {
  return Get-Els | Where-Object {
    $_.Current.Name -eq $Name -and $_.Current.ControlType.ProgrammaticName -ne 'ControlType.Text'
  } | Select-Object -First 1
}
function Dump-Matches([string]$Pattern) {
  return @(Get-Els | Where-Object { $_.Current.Name -like $Pattern } | ForEach-Object {
    "$($_.Current.ControlType.ProgrammaticName)|$($_.Current.Name)|$([int]$_.Current.BoundingRectangle.Y)|$([int]$_.Current.BoundingRectangle.Width)"
  })
}
function Activate($el) {
  if (-not $el) { return 'missing' }
  $w=Get-Win; if($w){ [W5]::SetForegroundWindow([IntPtr]$w.Current.NativeWindowHandle) | Out-Null }
  $sp=$null; if ($el.TryGetCurrentPattern([System.Windows.Automation.ScrollItemPattern]::Pattern,[ref]$sp)) { $sp.ScrollIntoView(); Start-Sleep -Milliseconds 500 }
  $ip=$null
  if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern,[ref]$ip)) { $ip.Invoke(); return 'invoke' }
  $el.SetFocus(); Start-Sleep -Milliseconds 300
  [System.Windows.Forms.SendKeys]::SendWait(' ')
  return 'space'
}
function Run-Key { param($name) $v=Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name $name -ErrorAction SilentlyContinue; return [bool]$v }
function Task-Exists { schtasks /Query /TN DiskMasterAutostart *> $null; return ($LASTEXITCODE -eq 0) }
function Task-RunLevel {
  $xml = schtasks /Query /TN DiskMasterAutostart /XML 2>$null | Out-String
  if ($xml -match '<RunLevel>([^<]+)</RunLevel>') { return $Matches[1] }
  return $null
}
try {
  Step 'install'
  $setup=Join-Path $Share 'DiskMaster-setup.exe'
  $p=Start-Process -FilePath $setup -ArgumentList '/S' -PassThru
  $p.WaitForExit(180000) | Out-Null
  $Result.install.exitCode=$p.ExitCode
  Start-Sleep 6
  $exe=Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter 'DiskMaster.exe' -ErrorAction SilentlyContinue | Select-Object -First 1
  $Result.install.version=if($exe){$exe.VersionInfo.FileVersion}else{$null}
  if(-not $exe){throw 'installed exe not found'}

  Step 'launch'
  Start-Process -FilePath $exe.FullName -ArgumentList '--tab=settings' | Out-Null
  Start-Sleep 16
  $w=Get-Win; if($w){ [W5]::SetForegroundWindow([IntPtr]$w.Current.NativeWindowHandle) | Out-Null }
  Start-Sleep 1
  if ($w) {
    $r=$w.Current.BoundingRectangle
    [W5]::Wheel([int]($r.X + $r.Width/2), [int]($r.Y + $r.Height/2), -2400)
    Start-Sleep -Seconds 1
  }
  $Result.diagnostics = [ordered]@{
    autostart = Dump-Matches '*Запускать при входе*'
    admin = Dump-Matches '*Всегда запускать*'
    save = Dump-Matches '*Сохранить*'
  }
  Save-Shot 'u5-settings.png'

  Step 'autostart-on'
  $sw=Find-Switch 'Запускать при входе в Windows'
  $how=Activate $sw
  Start-Sleep 3
  $Result.steps += [ordered]@{step='autostart-on'; found=[bool]$sw; how=$how; runKey=(Run-Key 'DiskMaster'); task=(Task-Exists)}

  Step 'admin-on'
  $admin=Find-Switch 'Всегда запускать с правами администратора'
  $how=Activate $admin
  Start-Sleep 1
  $save=Find-Switch 'Сохранить'
  $howSave=Activate $save
  Start-Sleep 8
  $Result.steps += [ordered]@{step='admin-on'; found=[bool]$admin; how=$how; save=$howSave; runKey=(Run-Key 'DiskMaster'); task=(Task-Exists); runLevel=(Task-RunLevel)}
  Save-Shot 'u5-admin-on.png'

  Step 'admin-off'
  $admin=Find-Switch 'Всегда запускать с правами администратора'
  $how=Activate $admin
  Start-Sleep 1
  $save=Find-Switch 'Сохранить'
  $howSave=Activate $save
  Start-Sleep 8
  $Result.steps += [ordered]@{step='admin-off'; found=[bool]$admin; how=$how; save=$howSave; runKey=(Run-Key 'DiskMaster'); task=(Task-Exists)}
  Save-Shot 'u5-admin-off.png'

  Step 'done'
} catch { $Result.notes += "fatal: $($_.Exception.Message)"; Step 'fatal' } finally { Save-Result; Step 'finished' }
