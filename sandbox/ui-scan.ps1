# UI smoke test: start a cleanup scan, check the Stop button and results.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$App = Join-Path $Share 'app\DiskMaster.exe'
$Result = [ordered]@{ phase = 'init'; notes = @(); steps = @(); elements = @() }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  [IO.File]::WriteAllText((Join-Path $Share 'ui-scan.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'ui-scan-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
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

Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
$Root = [System.Windows.Automation.AutomationElement]::RootElement
$Cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ClassNameProperty, 'Tauri Window')

function Get-Win {
  return @($Root.FindAll([System.Windows.Automation.TreeScope]::Children, $Cond)) | Sort-Object { $_.Current.BoundingRectangle.Width } -Descending | Select-Object -First 1
}
function Get-Els { $w = Get-Win; if (-not $w) { return @() }; return @($w.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)) }
function Find-Exact([string]$Name) { return Get-Els | Where-Object { $_.Current.Name -eq $Name } | Select-Object -First 1 }
function Invoke-El($el) {
  if (-not $el) { return 'missing' }
  $p = $null
  if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke(); return 'invoke' }
  return 'none'
}

try {
  Step 'launch'
  $proc = Start-Process -FilePath $App -ArgumentList '--tab=cleanup' -PassThru
  Start-Sleep -Seconds 15
  $running = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
  $Result.steps += [ordered]@{ step = 'app-running'; ok = [bool]$running }
  if (-not $running) { throw 'app did not start' }

  Step 'start'
  $start = $null
  for ($attempt = 0; $attempt -lt 12 -and -not $start; $attempt++) {
    $start = Find-Exact 'Начать проверку'
    if (-not $start) { $start = Find-Exact 'Проверить снова' }
    if (-not $start) { Start-Sleep -Seconds 1 }
  }
  $Result.steps += [ordered]@{ step = 'start-button'; found = [bool]$start; how = (Invoke-El $start) }

  Start-Sleep -Seconds 3
  $Result.steps += [ordered]@{ step = 'stop-button'; found = [bool](Find-Exact 'Остановить') }

  Start-Sleep -Seconds 22
  $Result.steps += [ordered]@{ step = 'stop-after'; found = [bool](Find-Exact 'Остановить') }
  foreach ($el in (Get-Els)) {
    $name = $el.Current.Name
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    if ($name -match 'Освобожд|Найдено|Сканирован|файлов|Остановить|Проверк') {
      $Result.elements += [ordered]@{ type = $el.Current.ControlType.ProgrammaticName; name = $name }
    }
  }
  Save-Shot 'ui-scan.png'
  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
