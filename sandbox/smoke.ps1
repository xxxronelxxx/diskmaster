# Smoke test for Диск-Мастер inside Windows Sandbox.
# Test-only harness: the product itself ships no PowerShell.
$ErrorActionPreference = 'Continue'
$Share = 'C:\Users\WDAGUtilityAccount\Desktop\share'
$Result = [ordered]@{
  phase     = 'init'
  startedAt = (Get-Date).ToString('o')
  notes     = @()
  env       = [ordered]@{}
  app       = [ordered]@{}
  helper    = [ordered]@{}
  system    = [ordered]@{}
}

function Save-Result {
  $Result.updatedAt = (Get-Date).ToString('o')
  $json = $Result | ConvertTo-Json -Depth 8
  [IO.File]::WriteAllText((Join-Path $Share 'result.json'), $json, (New-Object System.Text.UTF8Encoding($false)))
  Add-Content -Path (Join-Path $Share 'progress.log') -Value ("{0} {1}" -f (Get-Date).ToString('HH:mm:ss'), $Result.phase)
}

function Step([string]$Name) {
  $Result.phase = $Name
  Save-Result
}

function Save-Shot([string]$Name) {
  try {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $bmp.Save((Join-Path $Share $Name), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose()
    $bmp.Dispose()
    $Result.env.screen = "$($bounds.Width)x$($bounds.Height)"
  } catch {
    $Result.notes += "screenshot $Name failed: $($_.Exception.Message)"
  }
}

try {
  Step 'env'

  $Result.env.computer = $env:COMPUTERNAME
  $Result.env.user = "$env:USERDOMAIN\$env:USERNAME"
  $Result.env.os = (Get-CimInstance Win32_OperatingSystem).Caption
  $Result.env.osVersion = [Environment]::OSVersion.VersionString
  $Result.env.psVersion = $PSVersionTable.PSVersion.ToString()
  $Result.env.systemRoot = $env:SystemRoot
  $Result.env.share = $Share

  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = New-Object Security.Principal.WindowsPrincipal($identity)
  $Result.env.isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  $groups = (& whoami /groups) -join "`n"
  $Result.env.integrity = (($groups -split "`n") | Select-String 'Mandatory Label' | Out-String).Trim()

  $uac = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' -ErrorAction SilentlyContinue
  $Result.env.uacEnableLua = $uac.EnableLUA
  $Result.env.uacConsentPromptBehaviorAdmin = $uac.ConsentPromptBehaviorAdmin
  $Result.env.uacPromptOnSecureDesktop = $uac.PromptOnSecureDesktop

  $wvKeys = @(
    'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
    'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
  )
  $wv = @()
  foreach ($key in $wvKeys) {
    if (Test-Path $key) { $wv += (Get-ItemProperty $key).pv }
  }
  $Result.env.webview2 = ($wv | Select-Object -First 1)
  $Result.env.webview2Folder = Test-Path 'C:\Program Files (x86)\Microsoft\EdgeWebView\Application'

  $Result.system.windowsOld = Test-Path 'C:\Windows.old'
  $Result.system.systemRestoreDisabled = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore' -ErrorAction SilentlyContinue).DisableSR
  try {
    $Result.system.restorePointCount = @(Get-CimInstance -Namespace 'root/default' -ClassName 'SystemRestore' -ErrorAction Stop).Count
  } catch {
    $Result.system.restorePointCount = "error: $($_.Exception.Message)"
  }
  $Result.system.vssService = (Get-Service VSS -ErrorAction SilentlyContinue).Status
  $Result.system.dismPresent = Test-Path "$env:SystemRoot\System32\dism.exe"
  $Result.system.disk = Get-CimInstance Win32_LogicalDisk -Filter 'DeviceID="C:"' | ForEach-Object {
    "{0:N1} GB free of {1:N1} GB" -f ($_.FreeSpace / 1GB), ($_.Size / 1GB)
  }
  Step 'env-done'

  $appPath = Join-Path $Share 'app\DiskMaster.exe'
  $Result.app.path = $appPath
  $Result.app.exists = Test-Path $appPath
  if ($Result.app.exists) {
    $Result.app.version = (Get-Item $appPath).VersionInfo.FileVersion
    $helperPath = Join-Path $Share 'app\resources\DiskMasterHelper.exe'
    $Result.app.helperExists = Test-Path $helperPath
    if ($Result.app.helperExists) {
      $Result.app.helperVersion = (Get-Item $helperPath).VersionInfo.FileVersion
    }

    Step 'app-launch'
    $proc = Start-Process -FilePath $appPath -PassThru
    Start-Sleep -Seconds 18
    $running = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
    $Result.app.running = [bool]$running
    if ($running) {
      $Result.app.title = $running.MainWindowTitle
      $Result.app.memoryMB = [math]::Round($running.WorkingSet64 / 1MB, 1)
    } else {
      $Result.app.exitCode = $proc.ExitCode
    }
    $dataDir = Join-Path $Share 'app\disk-master-data'
    $Result.app.dataDir = Test-Path $dataDir
    if ($Result.app.dataDir) {
      $Result.app.dataFiles = @(Get-ChildItem $dataDir -Recurse -ErrorAction SilentlyContinue | Select-Object -First 30 -ExpandProperty Name)
    }
    Save-Shot 'app.png'
    Step 'app-shot'

    if ($running) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 2
  }

  $helper = Join-Path $Share 'app\resources\DiskMasterHelper.exe'
  if (Test-Path $helper) {
    Step 'helper-unelevated'
    $plainOut = Join-Path $Share 'helper-unelevated.json'
    $localOut = 'C:\Temp\helper-unelevated.json'
    New-Item -ItemType Directory -Path 'C:\Temp' -Force | Out-Null
    Remove-Item $plainOut, $localOut -Force -ErrorAction SilentlyContinue

    $watch = Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', 'ping -n 40 127.0.0.1 >nul & taskkill /f /im DiskMasterHelper.exe >nul 2>&1' -WindowStyle Hidden -PassThru
    Step 'helper-run'
    & $helper --op=restore_points_list "--output=$localOut" *> (Join-Path $Share 'helper-stdout.txt')
    $Result.helper.unelevatedExit = $LASTEXITCODE
    Stop-Process -Id $watch.Id -Force -ErrorAction SilentlyContinue
    Step 'helper-returned'

    if (Test-Path $localOut) {
      $Result.helper.unelevatedOutput = [IO.File]::ReadAllText($localOut)
      [IO.File]::WriteAllText($plainOut, $Result.helper.unelevatedOutput, (New-Object System.Text.UTF8Encoding($false)))
    }
    Step 'helper-unelevated-done'

    if ($Result.env.isAdmin) {
      $Result.helper.elevation = 'session already elevated; runas needs no consent'
    } else {
      Step 'helper-elevated'
      $elevOut = Join-Path $Share 'helper-elevated.json'
      $elevLocal = 'C:\Temp\helper-elevated.json'
      Remove-Item $elevOut, $elevLocal -Force -ErrorAction SilentlyContinue
      $childScript = "Start-Process -FilePath '$helper' -ArgumentList '--op=restore_points_list','--output=$elevLocal' -Verb RunAs -Wait"
      $child = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile', '-WindowStyle', 'Hidden', '-Command', $childScript -PassThru
      Start-Sleep -Seconds 4
      Save-Shot 'uac.png'
      $finished = $false
      try {
        Wait-Process -Id $child.Id -Timeout 20 -ErrorAction Stop
        $finished = $true
      } catch {
        $Result.notes += "runas child did not finish in 20s: $($_.Exception.Message)"
      }
      if (-not $finished) {
        Stop-Process -Id $child.Id -Force -ErrorAction SilentlyContinue
        $Result.helper.elevation = 'runas blocked (UAC consent prompt likely)'
      } else {
        $Result.helper.elevation = 'runas completed'
      }
      $Result.helper.elevatedOutput = if (Test-Path $elevLocal) {
        $elevText = [IO.File]::ReadAllText($elevLocal)
        [IO.File]::WriteAllText($elevOut, $elevText, (New-Object System.Text.UTF8Encoding($false)))
        $elevText
      } else { $null }
      Start-Sleep -Seconds 1
      Save-Shot 'after-uac.png'
    }
    Step 'helper-done'
  }

  Step 'done'
} catch {
  $Result.notes += "fatal: $($_.Exception.Message)"
  Step 'fatal'
} finally {
  $Result.notes += "share entries: " + ((Get-ChildItem $Share | Select-Object -ExpandProperty Name) -join ', ')
  Step 'finished'
}
