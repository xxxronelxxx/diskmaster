$items = New-Object System.Collections.ArrayList

$runKeys = @(
  @{ Path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'; Label = 'HKCU\Run' },
  @{ Path = 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Run'; Label = 'HKLM\Run' },
  @{ Path = 'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run'; Label = 'HKLM\Run (x86)' }
)

foreach ($key in $runKeys) {
  if (-not (Test-Path $key.Path)) { continue }
  $props = Get-ItemProperty -Path $key.Path
  foreach ($prop in $props.PSObject.Properties) {
    if ($prop.Name -like 'PS*') { continue }
    $value = [string]$prop.Value
    if ([string]::IsNullOrWhiteSpace($value)) { continue }
    $null = $items.Add([pscustomobject]@{
      id = ('reg|' + $key.Path + '|' + $prop.Name)
      name = $prop.Name
      command = $value
      location = $key.Label
      source = 'registry'
      enabled = $true
    })
  }
}

$approvedKeys = @(
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run',
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder',
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder'
)
$disabled = @{}
foreach ($key in $approvedKeys) {
  if (-not (Test-Path $key)) { continue }
  $props = Get-ItemProperty -Path $key
  foreach ($prop in $props.PSObject.Properties) {
    if ($prop.Name -like 'PS*') { continue }
    $bytes = $prop.Value
    if ($bytes -is [byte[]] -and $bytes.Length -gt 0) {
      if (($bytes[0] -band 1) -eq 1) { $disabled[$prop.Name] = $true }
    }
  }
}

$folders = @(
  @{ Path = [Environment]::GetFolderPath('Startup'); Label = 'Startup (пользователь)' },
  @{ Path = [Environment]::GetFolderPath('CommonStartup'); Label = 'Startup (все)' }
)
foreach ($folder in $folders) {
  if ([string]::IsNullOrWhiteSpace($folder.Path) -or -not (Test-Path $folder.Path)) { continue }
  foreach ($file in Get-ChildItem -Path $folder.Path -File -ErrorAction SilentlyContinue) {
    if ($file.Name -match 'desktop\.ini|\.log$') { continue }
    $isDisabled = $false
    if ($disabled.ContainsKey($file.Name)) { $isDisabled = $true }
    $null = $items.Add([pscustomobject]@{
      id = ('folder|' + $file.FullName)
      name = [System.IO.Path]::GetFileNameWithoutExtension($file.Name)
      command = $file.FullName
      location = $folder.Label
      source = 'startup-folder'
      enabled = (-not $isDisabled)
    })
  }
}

foreach ($regKey in Get-ItemProperty -Path $runKeys[0].Path, $runKeys[1].Path, $runKeys[2].Path -ErrorAction SilentlyContinue) {
  foreach ($prop in $regKey.PSObject.Properties) {
    if ($prop.Name -like 'PS*') { continue }
    foreach ($item in $items) {
      if ($item.source -eq 'registry' -and $item.name -eq $prop.Name -and $disabled.ContainsKey($prop.Name)) {
        $item.enabled = $false
      }
    }
  }
}

# Scheduled tasks that run at logon
Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object {
  $_.Triggers | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_TaskLogonTrigger' -or $_.CimClass.CimClassName -eq 'MSFT_TaskBootTrigger' }
} | ForEach-Object {
  $action = $_.Actions | Select-Object -First 1
  $null = $items.Add([pscustomobject]@{
    id = ('task|' + $_.TaskPath + '|' + $_.TaskName)
    name = $_.TaskName
    command = if ($action -and $action.Execute) { [string]$action.Execute } else { '' }
    location = $_.TaskPath
    source = 'task'
    enabled = ($_.State -ne 'Disabled')
  })
}

$result = @()
foreach ($item in $items) {
  $exe = $null
  $cmd = [string]$item.command
  if ($cmd -match '^\s*"([^"]+)"') { $exe = $matches[1] }
  elseif ($cmd -match '^\s*([^\s]+\.exe)') { $exe = $matches[1] }
  elseif ($cmd -match '\.exe') { $exe = ($cmd -split '\.exe')[0] + '.exe' }
  $resolved = $null
  $exists = $false
  if ($exe) {
    $resolved = [Environment]::ExpandEnvironmentVariables($exe)
    $exists = Test-Path -LiteralPath $resolved -ErrorAction SilentlyContinue
  }
  $publisher = $null
  if ($exists) {
    try { $publisher = (Get-Item -LiteralPath $resolved -ErrorAction Stop).VersionInfo.CompanyName } catch {}
  }
  $result += [pscustomobject]@{
    id = $item.id
    name = $item.name
    command = $item.command
    location = $item.location
    source = $item.source
    enabled = [bool]$item.enabled
    publisher = if ($publisher) { [string]$publisher } else { $null }
    executable = $resolved
    exists = [bool]$exists
    system = [bool]($item.source -eq 'task' -and ([string]$item.location).StartsWith('\Microsoft\Windows'))
  }
}
ConvertTo-Json -InputObject @($result) -Depth 4 -Compress
