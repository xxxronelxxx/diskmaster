# UI smoke test for the Drivers tab.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$App = Join-Path $Share 'app\DiskMaster.exe'
$Result = [ordered]@{ phase = 'init'; notes = @(); steps = @(); elements = @() }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  $json = $Result | ConvertTo-Json -Depth 8
  [IO.File]::WriteAllText((Join-Path $Share 'ui-drivers.json'), $json, (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'ui-drivers-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
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

function Get-Win {
  $root = [System.Windows.Automation.AutomationElement]::RootElement
  $cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Tauri Window')
  return @($root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)) | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1
}
function Get-Els { $w = Get-Win; if (-not $w) { return @() }; return @($w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) }
function Find-Exact([string]$Name) { return Get-Els | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1 }

try {
  Step 'launch'
  $proc = Start-Process -FilePath $App -ArgumentList '--tab=drivers' -PassThru
  Start-Sleep -Seconds 16
  $running = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
  $Result.steps += [ordered]@{ step = 'app-running'; ok = [bool]$running }
  if (-not $running) { throw 'app did not start' }

  Step 'drivers-tab'
  $win = Get-Win
  if (-not $win) { throw 'no Tauri window' }
  foreach ($el in (Get-Els)) {
    $name = $el.Current.Name
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    if ($name -eq 'Драйверы' -or $name -like '*Экспортировать*' -or $name -like '*Восстановить*' -or $name -like '*драйвер*' -or $name -like '*Опубликованное*') {
      $Result.elements += [ordered]@{ type = $el.Current.ControlType.ProgrammaticName; name = $name }
    }
  }
  $Result.steps += [ordered]@{ step = 'title'; found = [bool](Find-Exact 'Драйверы') }
  $Result.steps += [ordered]@{ step = 'export-all'; found = [bool](Find-Exact 'Экспортировать все') }
  $Result.steps += [ordered]@{ step = 'restore-folder'; found = [bool](Find-Exact 'Восстановить из папки') }
  Save-Shot 'ui-drivers.png'
  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
