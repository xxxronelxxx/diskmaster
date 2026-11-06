# SMART fallback test: helper op=smart and the maintenance-tab button.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$App = Join-Path $Share 'app\DiskMaster.exe'
$Helper = Join-Path $Share 'app\resources\DiskMasterHelper.exe'
$Result = [ordered]@{ phase = 'init'; startedAt = (Get-Date).ToString('o'); notes = @(); steps = @(); elements = @() }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  $json = $Result | ConvertTo-Json -Depth 8
  [IO.File]::WriteAllText((Join-Path $Share 'ui6.json'), $json, (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'ui6-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
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
public class M6 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, int data, UIntPtr extra);
  public static void Click(int x, int y) { SetForegroundWindow(IntPtr.Zero); SetCursorPos(x, y); mouse_event(0x02, 0, 0, 0, UIntPtr.Zero); mouse_event(0x04, 0, 0, 0, UIntPtr.Zero); }
}
'@
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

function Get-Win {
  $root = [System.Windows.Automation.AutomationElement]::RootElement
  $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Tauri Window')
  return @($root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1
}
function Get-Els { $w = Get-Win; if (-not $w) { return @() }; return @($w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) }
function Find-Exact([string]$Name) { return Get-Els | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1 }
function Invoke-El($el) {
  if (-not $el) { return 'missing' }
  $w = Get-Win
  if ($w) { [M6]::SetForegroundWindow([IntPtr]$w.Current.NativeWindowHandle) | Out-Null }
  $p = $null
  if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke(); return 'invoke' }
  $r = $el.Current.BoundingRectangle
  if ($r.Width -gt 0 -and $r.Height -gt 0) { [M6]::Click([int]($r.X + $r.Width / 2), [int]($r.Y + $r.Height / 2)); return 'click' }
  return 'none'
}
function Snapshot([string]$Tag) {
  foreach ($el in (Get-Els)) {
    $name = $el.Current.Name
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    if ($name -notlike '*SMART*' -and $name -notlike '*NVMe*' -and $name -notlike '*Здоровье*') { continue }
    $Result.elements += [ordered]@{ tag = $Tag; type = $el.Current.ControlType.ProgrammaticName; name = $name }
  }
}

try {
  Step 'helper-file'
  $Result.steps += [ordered]@{ step = 'helper-present'; ok = (Test-Path $Helper); path = $Helper }
  if (Test-Path $Helper) {
    $tmp = 'C:\Temp\smart-helper.json'
    New-Item -ItemType Directory -Force 'C:\Temp' | Out-Null
    Remove-Item $tmp -ErrorAction SilentlyContinue
    $p = Start-Process -FilePath $Helper -ArgumentList '--op=smart', "--output=$tmp" -PassThru -Wait
    $Result.steps += [ordered]@{ step = 'helper-smart'; exit = $p.ExitCode }
    if (Test-Path $tmp) {
      try {
        $data = Get-Content $tmp -Raw | ConvertFrom-Json
        $Result.steps += [ordered]@{
          step = 'helper-smart-json'
          ok = [bool]$data.ok
          message = $data.message
          disks = @($data.data).Count
        }
      } catch { $Result.notes += "helper json parse: $($_.Exception.Message)" }
      Copy-Item $tmp (Join-Path $Share 'smart-helper.json') -Force -ErrorAction SilentlyContinue
    } else {
      $Result.notes += 'helper produced no output file'
    }
  }

  Step 'launch'
  $proc = Start-Process -FilePath $App -ArgumentList '--tab=maintenance' -PassThru
  Start-Sleep -Seconds 16
  $running = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
  $Result.steps += [ordered]@{ step = 'app-running'; ok = [bool]$running }
  if (-not $running) { throw 'app did not start' }

  Step 'maintenance'
  $win = Get-Win
  if (-not $win) { throw 'no Tauri window' }
  [M6]::SetForegroundWindow([IntPtr]$win.Current.NativeWindowHandle) | Out-Null
  Start-Sleep -Seconds 2
  Snapshot 'before'
  Save-Shot 'ui6-before.png'

  $button = Find-Exact 'Прочитать SMART'
  if (-not $button) { $button = Find-Exact 'Прочитать SMART (UAC)' }
  if (-not $button) {
    $Result.notes += 'SMART button not found; trying to open maintenance via nav'
    $nav = Find-Exact 'Обслуживание'
    Invoke-El $nav | Out-Null
    Start-Sleep -Seconds 3
    $button = Find-Exact 'Прочитать SMART'
    if (-not $button) { $button = Find-Exact 'Прочитать SMART (UAC)' }
  }
  $Result.steps += [ordered]@{ step = 'smart-button'; found = [bool]$button }

  if ($button) {
    $how = Invoke-El $button
    $Result.steps += [ordered]@{ step = 'smart-click'; how = $how }
  } else {
    $Result.notes += 'SMART button unavailable'
  }
  Start-Sleep -Seconds 6
  Snapshot 'after'
  Save-Shot 'ui6-after.png'

  $hint = Find-Exact 'SMART не прочитан: NVMe-диск не найден или Windows не отдал журнал здоровья.'
  $Result.steps += [ordered]@{ step = 'fallback-hint'; found = [bool]$hint }
  $buttonAfter = Find-Exact 'Прочитать SMART'
  if (-not $buttonAfter) { $buttonAfter = Find-Exact 'Прочитать SMART (UAC)' }
  $Result.steps += [ordered]@{ step = 'button-still-there'; found = [bool]$buttonAfter }

  "SMART fallback checked" | Out-File (Join-Path $Share 'ui6-smart.txt') -Encoding utf8
  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
