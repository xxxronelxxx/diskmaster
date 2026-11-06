# UI test: drive the maintenance tab inside Windows Sandbox via UI Automation.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$App = Join-Path $Share 'app\DiskMaster.exe'
$Result = [ordered]@{ phase = 'init'; startedAt = (Get-Date).ToString('o'); notes = @(); steps = @(); elements = @() }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  $json = $Result | ConvertTo-Json -Depth 8
  [IO.File]::WriteAllText((Join-Path $Share 'ui.json'), $json, (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'ui-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }

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
public class Mouse {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  public static void Click(int x, int y) { SetCursorPos(x, y); mouse_event(0x02, 0, 0, 0, IntPtr.Zero); mouse_event(0x04, 0, 0, 0, IntPtr.Zero); }
}
'@
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Invoke-Element($el) {
  $pattern = $null
  if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
    return 'invoke'
  }
  $r = $el.Current.BoundingRectangle
  if ($r.Width -gt 0 -and $r.Height -gt 0) {
    [Mouse]::Click([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2))
    return 'click'
  }
  return 'none'
}

try {
  Step 'launch'
  $proc = Start-Process -FilePath $App -PassThru
  Start-Sleep -Seconds 15
  $running = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
  $Result.steps += [ordered]@{ step = 'app-running'; ok = [bool]$running; title = if ($running) { $running.MainWindowTitle } else { $null } }
  if (-not $running) { throw 'app did not start' }

  Step 'uia-tree'
  $root = [System.Windows.Automation.AutomationElement]::RootElement
  $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Tauri Window')
  $wins = @($root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond))
  $Result.notes += "tauri windows: $($wins.Count)"
  $win = $wins | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1
  if (-not $win) { throw 'no Tauri window in UIA tree' }
  $Result.steps += [ordered]@{ step = 'window'; bounds = $win.Current.BoundingRectangle.ToString() }

  $all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  foreach ($el in $all) {
    $name = $el.Current.Name
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    $Result.elements += [ordered]@{
      type = $el.Current.ControlType.ProgrammaticName
      name = $name
      id   = $el.Current.AutomationId
      x    = [int]$el.Current.BoundingRectangle.X
      y    = [int]$el.Current.BoundingRectangle.Y
    }
    if ($Result.elements.Count -ge 250) { break }
  }
  Step 'uia-done'

  $nav = $all | Where-Object { $_.Current.Name -eq 'Обслуживание' } | Select-Object -First 1
  if ($nav) {
    $how = Invoke-Element $nav
    $Result.steps += [ordered]@{ step = 'open-maintenance'; how = $how }
  } else {
    $Result.notes += 'nav not found in UIA; coordinate fallback'
    [Mouse]::Click(150, 360)
    $Result.steps += [ordered]@{ step = 'open-maintenance'; how = 'coordinate' }
  }
  Start-Sleep -Seconds 4
  Save-Shot 'ui-maint.png'
  Step 'maintenance-shot'

  $win2 = $wins | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1
  $all2 = $win2.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  $load = $all2 | Where-Object { $_.Current.Name -like '*Загрузить список*' } | Select-Object -First 1
  if ($load) {
    $how = Invoke-Element $load
    $Result.steps += [ordered]@{ step = 'load-points'; how = $how; name = $load.Current.Name }
  } else {
    $Result.notes += 'load button not found in UIA'
    $Result.steps += [ordered]@{ step = 'load-points'; how = 'missing' }
  }
  Start-Sleep -Seconds 8
  Save-Shot 'ui-loaded.png'
  Step 'load-shot'

  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
