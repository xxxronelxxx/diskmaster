# UI end-to-end: create a restore point through the app, then load the list via UAC.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$App = Join-Path $Share 'app\DiskMaster.exe'
$Result = [ordered]@{ phase = 'init'; startedAt = (Get-Date).ToString('o'); notes = @(); steps = @(); elements = @() }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  [IO.File]::WriteAllText((Join-Path $Share 'ui2.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'ui2-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
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
public class Mouse2 {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  public static void Click(int x, int y) { SetCursorPos(x, y); mouse_event(0x02, 0, 0, 0, IntPtr.Zero); mouse_event(0x04, 0, 0, 0, IntPtr.Zero); }
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
function Find-ByName([string]$Name) {
  $win = Get-AppWindow
  return Get-Elements $win | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1
}
function Invoke-Element($el) {
  if (-not $el) { return 'missing' }
  $pattern = $null
  if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    return 'invoke'
  }
  $r = $el.Current.BoundingRectangle
  if ($r.Width -gt 0 -and $r.Height -gt 0) {
    [Mouse2]::Click([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
    return 'click'
  }
  return 'none'
}
function Open-Maintenance {
  $win = Get-AppWindow
  $nav = Get-Elements $win | Where-Object { $_.Current.Name -eq 'Обслуживание' } | Select-Object -First 1
  if ($nav) { return (Invoke-Element $nav) }
  [Mouse2]::Click(150, 360)
  return 'coordinate'
}

try {
  Step 'enable-protection'
  try {
    Enable-ComputerRestore -Drive 'C:\' -ErrorAction Stop
    $Result.steps += [ordered]@{ step = 'enable-protection'; ok = $true }
  } catch {
    $Result.steps += [ordered]@{ step = 'enable-protection'; ok = $false; error = $_.Exception.Message }
  }

  Step 'launch'
  $proc = Start-Process -FilePath $App -PassThru
  Start-Sleep -Seconds 15
  $Result.steps += [ordered]@{ step = 'app-running'; ok = [bool](Get-Process -Id $proc.Id -ErrorAction SilentlyContinue) }
  $null = Get-Elements (Get-AppWindow)
  Start-Sleep -Seconds 2

  Step 'open-maintenance'
  $how = Open-Maintenance
  $Result.steps += [ordered]@{ step = 'open-maintenance'; how = $how }
  Start-Sleep -Seconds 3
  Save-Shot 'ui2-maint.png'

  # Make sure the tree is populated after navigation.
  $null = Get-Elements (Get-AppWindow)
  Start-Sleep -Seconds 1

  Step 'create-point'
  $create = Find-ByName 'Создать точку'
  $Result.steps += [ordered]@{ step = 'create-button'; found = [bool]$create }
  $how = Invoke-Element $create
  $Result.steps += [ordered]@{ step = 'create-click'; how = $how }
  Start-Sleep -Seconds 3
  Save-Shot 'ui2-modal.png'

  Step 'confirm'
  $confirm = Find-ByName 'Продолжить'
  if (-not $confirm) { $confirm = Find-ByName 'Продолжить ' }
  $how = Invoke-Element $confirm
  $Result.steps += [ordered]@{ step = 'confirm'; found = [bool]$confirm; how = $how }
  Start-Sleep -Seconds 12
  Save-Shot 'ui2-created.png'
  Step 'created'

  Step 'load-list'
  $load = Get-Elements (Get-AppWindow) | Where-Object { $_.Current.Name -like '*Загрузить список*' } | Select-Object -First 1
  $how = Invoke-Element $load
  $Result.steps += [ordered]@{ step = 'load-list'; found = [bool]$load; how = $how }
  Start-Sleep -Seconds 8
  Save-Shot 'ui2-list.png'
  Step 'list'

  $win = Get-AppWindow
  foreach ($el in (Get-Elements $win)) {
    $name = $el.Current.Name
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    $Result.elements += [ordered]@{ type = $el.Current.ControlType.ProgrammaticName; name = $name; id = $el.Current.AutomationId }
    if ($Result.elements.Count -ge 300) { break }
  }
  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
