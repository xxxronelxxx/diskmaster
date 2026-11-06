# Installs the NSIS build inside the sandbox and checks files + notifications.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$Result = [ordered]@{ phase = 'init'; startedAt = (Get-Date).ToString('o'); notes = @(); steps = @(); install = [ordered]@{} }

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  [IO.File]::WriteAllText((Join-Path $Share 'install.json'), ($Result | ConvertTo-Json -Depth 8), (New-Object System.Text.UTF8Encoding($false)))
}
function Step([string]$Name) { $Result.phase = $Name; Save-Result; Add-Content (Join-Path $Share 'install-progress.log') ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Name) }
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
  $Result.install.setupExists = Test-Path $setup
  $process = Start-Process -FilePath $setup -ArgumentList '/S' -PassThru
  $process.WaitForExit(120000) | Out-Null
  $Result.install.exitCode = $process.ExitCode
  Start-Sleep -Seconds 6

  Step 'locate'
  $exe = Get-ChildItem $env:LOCALAPPDATA -Recurse -Filter 'DiskMaster.exe' -ErrorAction SilentlyContinue |
    Select-Object -First 1
  $Result.install.path = if ($exe) { $exe.FullName } else { $null }
  $Result.install.version = if ($exe) { $exe.VersionInfo.FileVersion } else { $null }
  $Result.install.helper = if ($exe) { Test-Path (Join-Path $exe.DirectoryName 'resources\DiskMasterHelper.exe') } else { $false }
  $Result.install.files = if ($exe) { @(Get-ChildItem $exe.DirectoryName -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 40 | ForEach-Object { $_.FullName.Substring($exe.DirectoryName.Length) }) } else { @() }
  $dataDir = Join-Path $env:LOCALAPPDATA 'DiskMasterData'
  $Result.install.dataDir = Test-Path $dataDir
  $shortcuts = @(Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter '*.lnk' -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -like '*DiskMaster*' })
  $Result.install.shortcuts = @($shortcuts | ForEach-Object { $_.FullName })

  if (-not $exe) {
    throw 'installed executable not found'
  }

  Step 'launch'
  Stop-Process -Name DiskMaster -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
  Start-Process -FilePath $exe.FullName -ArgumentList '--tab=settings' | Out-Null
  Start-Sleep -Seconds 16
  $null = Get-AppWindow
  Save-Shot 'install-settings.png'
  $Result.steps += [ordered]@{ step = 'settings-tab'; ok = [bool](Find-Like '*Порог свободного места*') }

  Step 'notify'
  $test = Find-Named 'Проверить'
  $how = Invoke-Element $test
  Start-Sleep -Seconds 5
  Save-Shot 'install-toast.png'
  $Result.steps += [ordered]@{ step = 'notification'; found = [bool]$test; how = $how }
  $Result.steps += [ordered]@{ step = 'notification-result'; text = if (Find-Like '*Уведомление отправлено*') { 'sent' } elseif (Find-Like '*Не удалось*') { 'failed' } else { 'unknown' } }

  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  Save-Result
  Step 'finished'
}
