# UI smoke test: Overview dashboard and Ctrl+K palette.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$App = Join-Path $Share 'app\DiskMaster.exe'
$Result = [ordered]@{ phase = 'init'; notes = @(); steps = @(); elements = @() }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  [IO.File]::WriteAllText((Join-Path $Share 'ui-dashboard.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'ui-dashboard-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
function Save-Shot([string]$Name) {
  try {
    Add-Type -AssemblyName System.Windows.Forms,System.Drawing
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $bmp.Save((Join-Path $Share $Name), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
  } catch { $Result.notes += "shot ${Name}: $($_.Exception.Message)" }
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$Root = [System.Windows.Automation.AutomationElement]::RootElement
$Cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Tauri Window')

function Get-Win {
  return @($Root.FindAll([System.Windows.Automation.TreeScope]::Children, $Cond)) | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1
}
function Get-Els { $w = Get-Win; if (-not $w) { return @() }; return @($w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) }
function Find-Exact([string]$Name) { return Get-Els | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1 }

try {
  Step 'launch'
  $proc = Start-Process -FilePath $App -ArgumentList '--tab=overview' -PassThru
  Start-Sleep -Seconds 16
  $running = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
  $Result.steps += [ordered]@{ step = 'app-running'; ok = [bool]$running }
  if (-not $running) { throw 'app did not start' }

  Step 'overview'
  $win = Get-Win
  if (-not $win) { throw 'no Tauri window' }
  [System.Windows.Forms.SendKeys]::SendWait('%{TAB}') | Out-Null
  Start-Sleep -Milliseconds 300
  $Result.steps += [ordered]@{ step = 'health-card'; found = [bool](Find-Exact 'Здоровье дисков') }
  $Result.steps += [ordered]@{ step = 'cleanup-card'; found = [bool](Find-Exact 'Очистка за 30 дней') }
  Save-Shot 'ui-dashboard-overview.png'

  Step 'palette'
  [System.Windows.Forms.SendKeys]::SendWait('^k') | Out-Null
  Start-Sleep -Seconds 1
  $edit = Get-Els | Where-Object { $_.Current.ControlType.ProgrammaticName -eq 'ControlType.Edit' } | Select-Object -First 1
  $Result.steps += [ordered]@{ step = 'palette-edit'; found = [bool]$edit }
  foreach ($el in (Get-Els)) {
    $name = $el.Current.Name
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    if ($name -like '*Поиск по разделам*' -or $name -eq 'Действия' -or $name -eq 'Драйверы' -or $name -eq 'Настройки') {
      $Result.elements += [ordered]@{ type = $el.Current.ControlType.ProgrammaticName; name = $name }
    }
  }
  Save-Shot 'ui-dashboard-palette.png'
  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
