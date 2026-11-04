param(
  [string]$SettingsFile = "",
  [string]$SelfTest = ""
)

$ErrorActionPreference = 'Stop'
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($SettingsFile)) { $SettingsFile = Join-Path $scriptDir 'widget-settings.json' }
$logFile = Join-Path $scriptDir 'widget.log'

function Write-Log([string]$text) {
  try { Add-Content -Path $logFile -Value ((Get-Date).ToString('HH:mm:ss') + '  ' + $text) -Encoding UTF8 } catch {}
}

try {
  Add-Type -AssemblyName PresentationFramework
  Add-Type -AssemblyName PresentationCore
  Add-Type -AssemblyName WindowsBase
  Add-Type -AssemblyName System.Windows.Forms
  Add-Type -AssemblyName System.Drawing

  $signature = @'
using System;
using System.Runtime.InteropServices;
public class WidgetNative {
  [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr SetWindowPos(IntPtr hWnd, IntPtr after, int X, int Y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hWnd, int index);
  [DllImport("user32.dll")] public static extern int SetWindowLong(IntPtr hWnd, int index, int value);
  [DllImport("user32.dll", CharSet=CharSet.Auto)] public static extern IntPtr FindWindow(string cls, string win);
  [DllImport("user32.dll")] public static extern IntPtr SetParent(IntPtr child, IntPtr parent);
  [DllImport("user32.dll")] public static extern bool SetWindowText(IntPtr hWnd, string text);
}
'@
  Add-Type -TypeDefinition $signature

  function Get-Settings {
    $defaults = [ordered]@{
      x = -1; y = -1; width = 320; height = 210
      opacity = 0.82; accent = '#38e0a0'; radius = 14; fontScale = 1.0
      showCpu = $true; showRam = $true; showDisk = $true; showNet = $true
      showDrives = $true; showTopProcess = $false
      bars = $true; zMode = 'normal'; clickThrough = $false
      intervalMs = 1200; showTitle = $true; title = 'ДИСК-МАСТЕР'
    }
    if (-not (Test-Path $SettingsFile)) { return [pscustomobject]$defaults }
    try {
      $raw = Get-Content -Path $SettingsFile -Raw -Encoding UTF8 | ConvertFrom-Json
      # Older builds called this option "sparklines"; carry it over.
      if ($null -ne $raw.sparklines -and $null -eq $raw.bars) {
        $raw | Add-Member -NotePropertyName bars -NotePropertyValue ([bool]$raw.sparklines) -Force
      }
      foreach ($key in $defaults.Keys) {
        if ($null -eq $raw.$key) { $raw | Add-Member -NotePropertyName $key -NotePropertyValue $defaults[$key] -Force }
      }
      return $raw
    } catch {
      Write-Log ('settings parse error: ' + $_.Exception.Message)
      return [pscustomobject]$defaults
    }
  }

  function Save-WindowState($win) {
    try {
      $current = Get-Settings
      $current.x = [int]$win.Left
      $current.y = [int]$win.Top
      $current.width = [int]$win.Width
      $current.height = [int]$win.Height
      $json = $current | ConvertTo-Json -Depth 4
      # Write UTF-8 without a BOM: the Node side parses this file and
      # JSON.parse rejects a leading BOM, which used to reset all settings.
      [System.IO.File]::WriteAllText($SettingsFile, $json, (New-Object System.Text.UTF8Encoding $false))
    } catch { Write-Log ('save state error: ' + $_.Exception.Message) }
  }

  function ConvertTo-Brush([string]$hex, [double]$alpha) {
    $value = $hex.TrimStart('#')
    if ($value.Length -eq 6) {
      $r = [Convert]::ToInt32($value.Substring(0, 2), 16)
      $g = [Convert]::ToInt32($value.Substring(2, 2), 16)
      $b = [Convert]::ToInt32($value.Substring(4, 2), 16)
    } else {
      $r = 56; $g = 224; $b = 160
    }
    $brush = New-Object System.Windows.Media.SolidColorBrush
    $brush.Color = [System.Windows.Media.Color]::FromArgb([int]($alpha * 255), $r, $g, $b)
    return $brush
  }

  function Format-Bytes([double]$bytes) {
    if ($bytes -ge 1073741824) { return ('{0:N1} ГБ' -f ($bytes / 1073741824)) }
    if ($bytes -ge 1048576) { return ('{0:N0} МБ' -f ($bytes / 1048576)) }
    if ($bytes -ge 1024) { return ('{0:N0} КБ' -f ($bytes / 1024)) }
    return ('{0:N0} Б' -f $bytes)
  }

  function New-TextBlock([double]$size, $brush, [string]$weight) {
    $tb = New-Object System.Windows.Controls.TextBlock
    $tb.FontSize = $size
    $tb.Foreground = $brush
    $tb.FontFamily = New-Object System.Windows.Media.FontFamily('Segoe UI')
    if ($weight -eq 'bold') { $tb.FontWeight = [System.Windows.FontWeights]::SemiBold }
    $tb.Text = ''
    return $tb
  }

  $settings = Get-Settings
  Write-Log ('starting widget, zMode=' + $settings.zMode + ' size=' + $settings.width + 'x' + $settings.height)

  $script:accentBrush = ConvertTo-Brush $settings.accent 1.0
  $script:dimBrush = New-Object System.Windows.Media.SolidColorBrush ([System.Windows.Media.Color]::FromArgb(255, 150, 165, 185))
  $script:whiteBrush = New-Object System.Windows.Media.SolidColorBrush ([System.Windows.Media.Color]::FromArgb(255, 236, 244, 252))
  $script:scale = [double]$settings.fontScale

  $win = New-Object System.Windows.Window
  $win.WindowStyle = [System.Windows.WindowStyle]::None
  $win.AllowsTransparency = $true
  $win.Background = [System.Windows.Media.Brushes]::Transparent
  $win.ShowInTaskbar = $false
  $win.ResizeMode = [System.Windows.ResizeMode]::CanResize
  $win.Width = [double]$settings.width
  $win.Height = [double]$settings.height
  $win.Title = 'Диск-Мастер — мониторинг'

  $screen = [System.Windows.SystemParameters]::WorkArea
  if ([double]$settings.x -lt 0 -or [double]$settings.y -lt 0) {
    $win.Left = $screen.Right - $win.Width - 28
    $win.Top = $screen.Top + 28
  } else {
    $win.Left = [double]$settings.x
    $win.Top = [double]$settings.y
  }

  $card = New-Object System.Windows.Controls.Border
  $card.CornerRadius = New-Object System.Windows.CornerRadius ([double]$settings.radius)
  $card.Background = ConvertTo-Brush '#0b1220' ([double]$settings.opacity)
  $card.BorderBrush = ConvertTo-Brush $settings.accent 0.28
  $card.BorderThickness = New-Object System.Windows.Thickness 1
  $card.Padding = New-Object System.Windows.Thickness 12
  $win.Content = $card

  $stack = New-Object System.Windows.Controls.StackPanel

  $header = New-Object System.Windows.Controls.TextBlock
  $header.Text = [string]$settings.title
  $header.FontSize = 10 * $script:scale
  $header.Foreground = ConvertTo-Brush $settings.accent 0.85
  $header.FontFamily = New-Object System.Windows.Media.FontFamily('Segoe UI')
  $header.FontWeight = [System.Windows.FontWeights]::SemiBold
  $header.Margin = New-Object System.Windows.Thickness 0, 0, 0, 6
  if ($settings.showTitle) { [void]$stack.Children.Add($header) }

  $script:rows = @{}
  $script:bars = @{}

  function Add-MetricRow([string]$key, [string]$label) {
    $rowGrid = New-Object System.Windows.Controls.Grid
    $rowGrid.Margin = New-Object System.Windows.Thickness 0, 1, 0, 1
    $col1 = New-Object System.Windows.Controls.ColumnDefinition
    $col1.Width = New-Object System.Windows.GridLength 46
    $col2 = New-Object System.Windows.Controls.ColumnDefinition
    $col2.Width = New-Object System.Windows.GridLength ([double]1, [System.Windows.GridUnitType]::Star)
    $col3 = New-Object System.Windows.Controls.ColumnDefinition
    $col3.Width = New-Object System.Windows.GridLength 78
    [void]$rowGrid.ColumnDefinitions.Add($col1)
    [void]$rowGrid.ColumnDefinitions.Add($col2)
    [void]$rowGrid.ColumnDefinitions.Add($col3)

    $labelBlock = New-TextBlock (9 * $script:scale) $script:dimBrush 'normal'
    $labelBlock.Text = $label
    $labelBlock.Margin = New-Object System.Windows.Thickness 0, 3, 0, 0
    [System.Windows.Controls.Grid]::SetColumn($labelBlock, 0)

    $valueBlock = New-TextBlock (10 * $script:scale) $script:whiteBrush 'bold'
    $valueBlock.Text = '—'
    $valueBlock.TextAlignment = [System.Windows.TextAlignment]::Right
    $valueBlock.Margin = New-Object System.Windows.Thickness 0, 2, 0, 0
    [System.Windows.Controls.Grid]::SetColumn($valueBlock, 2)

    $visual = $null
    if ($settings.bars) {
      # Horizontal indicator bar: a rounded track with a filled portion,
      # matching the preview shown in the app settings.
      $track = New-Object System.Windows.Controls.Grid
      $track.Height = 5
      $track.Margin = New-Object System.Windows.Thickness 8, 0, 10, 0
      $track.VerticalAlignment = [System.Windows.VerticalAlignment]::Center
      $track.ClipToBounds = $true

      # The filled part is a plain border whose width is set in pixels on every
      # tick; star-sized columns collapse unpredictably inside a few-pixel row.
      $fill = New-Object System.Windows.Controls.Border
      $fill.CornerRadius = New-Object System.Windows.CornerRadius 2.5
      $fill.Background = ConvertTo-Brush $settings.accent 0.75
      $fill.HorizontalAlignment = [System.Windows.HorizontalAlignment]::Left
      $fill.Width = 0
      $track.Children.Add($fill)

      [System.Windows.Controls.Grid]::SetColumn($track, 1)
      [void]$rowGrid.Children.Add($track)
      $visual = $track
      $script:bars[$key] = @{ fill = $fill; track = $track }
    } else {
      $valueBlock.Margin = New-Object System.Windows.Thickness 0, 2, 0, 0
      [System.Windows.Controls.Grid]::SetColumn($valueBlock, 1)
      $valueBlock.TextAlignment = [System.Windows.TextAlignment]::Right
      [System.Windows.Controls.Grid]::SetColumnSpan($valueBlock, 2)
    }

    [void]$rowGrid.Children.Add($labelBlock)
    [void]$rowGrid.Children.Add($valueBlock)
    [void]$stack.Children.Add($rowGrid)
    $script:rows[$key] = @{ value = $valueBlock; bar = $visual }
  }

  if ($settings.showCpu) { Add-MetricRow 'cpu' 'CPU' }
  if ($settings.showRam) { Add-MetricRow 'ram' 'ОЗУ' }
  if ($settings.showDisk) { Add-MetricRow 'disk' 'ДИСК' }
  if ($settings.showNet) { Add-MetricRow 'net' 'СЕТЬ' }
  if ($settings.showDrives) { Add-MetricRow 'drive' 'СВОБОДНО' }
  if ($settings.showTopProcess) { Add-MetricRow 'proc' 'ТОП' }

  function Update-Bar([string]$key, [double]$value) {
    if (-not $script:bars.ContainsKey($key)) { return }
    $clamped = [math]::Max(0, [math]::Min(100, $value))
    $entry = $script:bars[$key]
    $available = $entry.track.ActualWidth
    if ($available -le 0) { $available = 150 }
    $entry.fill.Width = [math]::Max(0, ($available * $clamped / 100))
  }

  $prevDiskR = $null
  $prevDiskW = $null
  $prevNet = @{}
  $prevMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()

  $script:driveLetter = $null
  try {
    $sysDrive = ($env:SystemDrive -replace ':', '')
    $script:driveLetter = $sysDrive
  } catch {}

  function Get-Metrics {
    $result = @{ ok = $false }
    try {
      $nowMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
      $delta = $nowMs - $prevMs
      if ($delta -le 0) { $delta = 1000 }

      $osData = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
      $cpuData = @(Get-CimInstance Win32_PerfFormattedData_PerfOS_Processor -ErrorAction SilentlyContinue)
      $total = 0
      foreach ($c in $cpuData) {
        if ($c.Name -notmatch '^\d+$') { $total = [int]$c.PercentProcessorTime }
      }
      if ($total -gt 100) { $total = 100 }

      $sumR = 0.0
      $sumW = 0.0
      foreach ($d in @(Get-CimInstance Win32_PerfRawData_PerfDisk_PhysicalDisk -ErrorAction SilentlyContinue)) {
        $sumR += [double]$d.DiskReadBytesPersec
        $sumW += [double]$d.DiskWriteBytesPersec
      }
      $diskIn = 0.0
      $diskOut = 0.0
      if ($null -ne $prevDiskR) {
        $diskIn = [math]::Max(0, ($sumR - $prevDiskR) / $delta * 1000)
        $diskOut = [math]::Max(0, ($sumW - $prevDiskW) / $delta * 1000)
      }

      $netIn = 0.0
      $netOut = 0.0
      $nextNet = @{}
      foreach ($n in @(Get-CimInstance Win32_PerfRawData_Tcpip_NetworkInterface -ErrorAction SilentlyContinue)) {
        $name = [string]$n.Name
        if ($name -match 'Loopback|ISATAP|Teredo|Pseudo') { continue }
        $recv = [double]$n.BytesReceivedPersec
        $sent = [double]$n.BytesSentPersec
        $nextNet[$name] = @{ r = $recv; s = $sent }
        if ($prevNet.ContainsKey($name)) {
          $netIn += [math]::Max(0, ($recv - $prevNet[$name].r) / $delta * 1000)
          $netOut += [math]::Max(0, ($sent - $prevNet[$name].s) / $delta * 1000)
        }
      }

      $result = @{
        ok = $true
        cpu = [double]$total
        ramUsed = ([double]$osData.TotalVisibleMemorySize - [double]$osData.FreePhysicalMemory) * 1024
        ramTotal = [double]$osData.TotalVisibleMemorySize * 1024
        diskIn = $diskIn
        diskOut = $diskOut
        netIn = $netIn
        netOut = $netOut
      }

      if ($settings.showDrives) {
        $drive = Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' -ErrorAction SilentlyContinue |
          Where-Object { $_.DeviceID -eq ($script:driveLetter + ':') } |
          Select-Object -First 1
        if ($null -ne $drive -and $drive.Size -gt 0) {
          $result.driveFree = [double]$drive.FreeSpace
          $result.driveTotal = [double]$drive.Size
        }
      }

      if ($settings.showTopProcess) {
        $proc = Get-Process -ErrorAction SilentlyContinue | Sort-Object WorkingSet64 -Descending | Select-Object -First 1
        if ($null -ne $proc) {
          $result.procName = [string]$proc.ProcessName
          $result.procMemory = [double]$proc.WorkingSet64
        }
      }

      $script:prevDiskR = $sumR
      $script:prevDiskW = $sumW
      $script:prevNet = $nextNet
      $script:prevMs = $nowMs
    } catch {
      Write-Log ('metrics error: ' + $_.Exception.Message)
    }
    return $result
  }

  $script:prevDiskR = $null
  $script:prevDiskW = $null
  $script:prevNet = @{}

  function Update-Ui {
    $m = Get-Metrics
    if (-not $m.ok) {
      if ($script:rows.ContainsKey('cpu')) { $script:rows['cpu'].value.Text = 'нет данных' }
      return
    }

    if ($script:rows.ContainsKey('cpu')) {
      $script:rows['cpu'].value.Text = ('{0:N0}%' -f $m.cpu)
      Update-Bar 'cpu' $m.cpu
    }
    if ($script:rows.ContainsKey('ram')) {
      $ramPercent = 0
      if ($m.ramTotal -gt 0) { $ramPercent = ($m.ramUsed / $m.ramTotal) * 100 }
      $script:rows['ram'].value.Text = ((Format-Bytes $m.ramUsed) + '  ' + ('{0:N0}%' -f $ramPercent))
      Update-Bar 'ram' $ramPercent
    }
    if ($script:rows.ContainsKey('disk')) {
      $totalIo = $m.diskIn + $m.diskOut
      $diskPercent = [math]::Min(100, ($totalIo / 104857600) * 100)
      $script:rows['disk'].value.Text = ('R ' + (Format-Bytes $m.diskIn) + '  W ' + (Format-Bytes $m.diskOut))
      Update-Bar 'disk' $diskPercent
    }
    if ($script:rows.ContainsKey('net')) {
      $totalNet = $m.netIn + $m.netOut
      $netPercent = [math]::Min(100, ($totalNet / 10485760) * 100)
      $script:rows['net'].value.Text = ('↓' + (Format-Bytes $m.netIn) + '  ↑' + (Format-Bytes $m.netOut))
      Update-Bar 'net' $netPercent
    }
    if ($script:rows.ContainsKey('drive') -and $m.ContainsKey('driveTotal')) {
      $usedPercent = 0
      if ($m.driveTotal -gt 0) { $usedPercent = (($m.driveTotal - $m.driveFree) / $m.driveTotal) * 100 }
      $script:rows['drive'].value.Text = ((Format-Bytes $m.driveFree) + ' свободно')
      Update-Bar 'drive' $usedPercent
    }
    if ($script:rows.ContainsKey('proc') -and $m.ContainsKey('procName')) {
      $script:rows['proc'].value.Text = ($m.procName + ' ' + (Format-Bytes $m.procMemory))
    }
  }

  $menu = New-Object System.Windows.Controls.ContextMenu
  $miRefresh = New-Object System.Windows.Controls.MenuItem
  $miRefresh.Header = 'Обновить метрики'
  $miRefresh.Add_Click({ Update-Ui })
  [void]$menu.Items.Add($miRefresh)

  $miHide = New-Object System.Windows.Controls.MenuItem
  $miHide.Header = 'Скрыть на 10 минут'
  $miHide.Add_Click({ $win.Hide(); $script:hideUntil = (Get-Date).AddMinutes(10) })
  [void]$menu.Items.Add($miHide)

  $miTray = New-Object System.Windows.Controls.MenuItem
  $miTray.Header = 'Свернуть в трей'
  $miTray.Add_Click({ $win.Hide() })
  [void]$menu.Items.Add($miTray)

  $miExit = New-Object System.Windows.Controls.MenuItem
  $miExit.Header = 'Закрыть виджет'
  $miExit.Add_Click({ Save-WindowState $win; $win.Close() })
  [void]$menu.Items.Add($miExit)
  $card.ContextMenu = $menu

  # No resize grip: a borderless window with ResizeMode=CanResize can be
  # stretched by its edges, which is discoverable enough and leaves the
  # surface clean.
  $card.Child = $stack

  $win.Add_MouseLeftButtonDown({ try { $win.DragMove() } catch {} })
  $win.Add_MouseDoubleClick({ Save-WindowState $win })
  $win.Add_Closing({ Save-WindowState $win })

  $script:hideUntil = $null

  # Tray icon keeps the widget reachable even when hidden or click-through.
  $tray = New-Object System.Windows.Forms.NotifyIcon
  $tray.Icon = [System.Drawing.SystemIcons]::Application
  $tray.Text = 'Диск-Мастер — мониторинг'
  $tray.Visible = $true
  $trayMenu = New-Object System.Windows.Forms.ContextMenuStrip
  $trayShow = $trayMenu.Items.Add('Показать виджет')
  $trayShow.Add_Click({
    $win.Show()
    $win.Activate()
    if ($settings.zMode -eq 'top') { $win.Topmost = $true }
  })
  $trayHide = $trayMenu.Items.Add('Скрыть')
  $trayHide.Add_Click({ $win.Hide() })
  [void]$trayMenu.Items.Add((New-Object System.Windows.Forms.ToolStripSeparator))
  $trayExit = $trayMenu.Items.Add('Закрыть виджет')
  $trayExit.Add_Click({ Save-WindowState $win; $win.Close() })
  $tray.ContextMenuStrip = $trayMenu
  $tray.Add_MouseDoubleClick({
    if ($win.IsVisible) { $win.Hide() } else { $win.Show() }
  })

  $win.Add_Closed({
    try { $tray.Visible = $false; $tray.Dispose() } catch {}
  })

  $timer = New-Object System.Windows.Threading.DispatcherTimer
  $timer.Interval = [TimeSpan]::FromMilliseconds([math]::Max(500, [int]$settings.intervalMs))
  $timer.Add_Tick({
    try {
      if ($null -ne $script:hideUntil) {
        if ((Get-Date) -gt $script:hideUntil) { $script:hideUntil = $null; $win.Show() }
      }
      Update-Ui
    } catch { Write-Log ('tick error: ' + $_.Exception.Message) }
  })

  $win.Add_SourceInitialized({
    $helper = New-Object System.Windows.Interop.WindowInteropHelper($win)
    $hwnd = $helper.Handle
    $script:hwnd = $hwnd
    $exStyle = [WidgetNative]::GetWindowLong($hwnd, -20)
    $toolWindow = 0x00000080
    $noActivate = 0x08000000
    [void][WidgetNative]::SetWindowLong($hwnd, -20, ($exStyle -bor $toolWindow))

    if ($settings.clickThrough) {
      $current = [WidgetNative]::GetWindowLong($hwnd, -20)
      [void][WidgetNative]::SetWindowLong($hwnd, -20, ($current -bor 0x00000020 -bor $noActivate))
    }

    if ($settings.zMode -eq 'top') {
      $win.Topmost = $true
    } elseif ($settings.zMode -eq 'desktop') {
      # Sink the window to the bottom of the z-order: it stays above the
      # wallpaper but never covers application windows.
      $hwndBottom = [IntPtr]::new(1)
      $flags = 0x0001 -bor 0x0002 -bor 0x0010  # NOSIZE | NOMOVE | NOACTIVATE
      [void][WidgetNative]::SetWindowPos($hwnd, $hwndBottom, 0, 0, 0, 0, $flags)
      Write-Log 'z-order set to bottom (desktop level)'
    } else {
      $win.Topmost = $false
    }
  })

  $win.Add_Loaded({
    try {
      Update-Ui
      $timer.Start()
    } catch { Write-Log ('load error: ' + $_.Exception.Message) }
  })

  if (-not [string]::IsNullOrWhiteSpace($SelfTest)) {
    $win.WindowStartupLocation = [System.Windows.WindowStartupLocation]::Manual
    $win.Left = -4000
    $win.Top = -4000
    $win.ShowInTaskbar = $false
    $win.Show()
    Update-Ui
    Update-Ui
    $win.Dispatcher.Invoke([System.Action]{}, [System.Windows.Threading.DispatcherPriority]::Render)
    $content = $win.Content
    $content.Measure((New-Object System.Windows.Size 4000, 4000))
    $content.Arrange((New-Object System.Windows.Rect 0, 0, $win.Width, $win.Height))
    $content.UpdateLayout()

    $report = [ordered]@{
      desired = @($stack.DesiredSize.Width, $stack.DesiredSize.Height)
      actual = @($stack.ActualWidth, $stack.ActualHeight)
      window = @($win.Width, $win.Height)
      rows = @()
      emptyValues = @()
    }
    foreach ($key in $script:rows.Keys) {
      $entry = $script:rows[$key]
      $barWidth = $null
      $trackWidth = $null
      if ($script:bars.ContainsKey($key)) {
        $barWidth = [math]::Round($script:bars[$key].fill.ActualWidth, 1)
        $trackWidth = [math]::Round($entry.bar.ActualWidth, 1)
      }
      $report.rows += [ordered]@{
        key = $key
        value = $entry.value.Text
        top = $entry.value.TranslatePoint((New-Object System.Windows.Point 0, 0), $content).Y
        width = $entry.value.ActualWidth
        barWidth = $barWidth
        trackWidth = $trackWidth
      }
      if ([string]::IsNullOrWhiteSpace($entry.value.Text) -or $entry.value.Text -eq '—') {
        $report.emptyValues += $key
      }
    }
    Write-Output ('REPORT ' + ($report | ConvertTo-Json -Depth 5 -Compress))
    try {
      $reportPath = [System.IO.Path]::ChangeExtension($SelfTest, '.json')
      $report | ConvertTo-Json -Depth 5 | Set-Content -Path $reportPath -Encoding UTF8
    } catch { Write-Log ('report write failed: ' + $_.Exception.Message) }

    $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap ([int]$win.Width), ([int]$win.Height), 96, 96, ([System.Windows.Media.PixelFormats]::Pbgra32)
    $rtb.Render($content)
    $encoder = New-Object System.Windows.Media.Imaging.PngBitmapEncoder
    $encoder.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
    $stream = [System.IO.File]::Create($SelfTest)
    $encoder.Save($stream)
    $stream.Close()
    Write-Log ('selftest written to ' + $SelfTest)
    $win.Close()
    return
  }

  # A saved position may point to a monitor that is no longer connected, which
  # would leave the widget invisible outside the desktop. Clamp it back on screen.
  $virtualLeft = [System.Windows.SystemParameters]::VirtualScreenLeft
  $virtualTop = [System.Windows.SystemParameters]::VirtualScreenTop
  $virtualWidth = [System.Windows.SystemParameters]::VirtualScreenWidth
  $virtualHeight = [System.Windows.SystemParameters]::VirtualScreenHeight

  $needsReposition = $false
  if ([double]$settings.x -lt 0 -and [double]$settings.y -lt 0) {
    $needsReposition = $true
  } elseif (
    $win.Left + 40 -lt $virtualLeft -or
    $win.Left + 40 -gt ($virtualLeft + $virtualWidth) -or
    $win.Top + 40 -lt $virtualTop -or
    $win.Top + 40 -gt ($virtualTop + $virtualHeight)
  ) {
    $needsReposition = $true
    Write-Log ('saved position ' + $settings.x + ',' + $settings.y + ' is off screen, moving to corner')
  }

  if ($needsReposition) {
    $work = [System.Windows.SystemParameters]::WorkArea
    $win.Left = $work.Right - $win.Width - 28
    $win.Top = $work.Top + 28
  }

  # Persist geometry when the user drags or resizes, not only when closing.
  $win.Add_LocationChanged({ try { Save-WindowState $win } catch {} })
  $win.Add_SizeChanged({ try { Save-WindowState $win } catch {} })

  Write-Log ('showing widget at ' + [int]$win.Left + ',' + [int]$win.Top + ' size ' + [int]$win.Width + 'x' + [int]$win.Height)

  # The window must be shown explicitly: Dispatcher.Run() alone keeps it hidden.
  $win.Show()
  $win.Activate()
  $win.Dispatcher.Invoke([System.Action]{}, [System.Windows.Threading.DispatcherPriority]::Render)

  # Height follows the content so no empty space is left under the last metric.
  # Width stays as configured, and the user can still stretch the window by its
  # edges during the session.
  try {
    $stack.Measure((New-Object System.Windows.Size([double]::PositiveInfinity, [double]::PositiveInfinity)))
    $contentHeight = $stack.DesiredSize.Height + 26  # card padding (12 top + 12 bottom) + border
    if ($contentHeight -gt 0) {
      $win.Height = [math]::Max(70, [math]::Ceiling($contentHeight))
      Write-Log ('height fitted to content: ' + [int]$win.Height)
    }
  } catch { Write-Log ('fit height error: ' + $_.Exception.Message) }

  [System.Windows.Threading.Dispatcher]::Run()
} catch {
  Write-Log ('fatal: ' + $_.Exception.Message + ' | ' + $_.ScriptStackTrace)
  if (-not [string]::IsNullOrWhiteSpace($SelfTest)) { Write-Output ('ERROR: ' + $_.Exception.Message) }
}
