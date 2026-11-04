try { [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false } catch {}
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$prevMs = $sw.ElapsedMilliseconds
$prevDiskR = $null
$prevDiskW = $null
$prevNet = @{}
while ($true) {
  try {
    $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
    $cpuData = @(Get-CimInstance Win32_PerfFormattedData_PerfOS_Processor -ErrorAction SilentlyContinue)
    $cores = @()
    $total = 0
    foreach ($c in $cpuData) {
      $v = [int]$c.PercentProcessorTime
      if ($v -gt 100) { $v = 100 }
      if ($c.Name -match '^\d+$') { $cores += ,$v } else { $total = $v }
    }
    if ($cores.Count -gt 0) {
      $cores = @($cores | Sort-Object)
      if ($total -le 0) {
        $sum = 0
        foreach ($v in $cores) { $sum += $v }
        $total = [int]($sum / $cores.Count)
      }
    }

    $diskRead = 0.0
    $diskWrite = 0.0
    $nowMs = $sw.ElapsedMilliseconds
    $delta = $nowMs - $prevMs
    if ($delta -le 0) { $delta = 1000 }
    $sumR = 0.0
    $sumW = 0.0
    $disks = @(Get-CimInstance Win32_PerfRawData_PerfDisk_PhysicalDisk -ErrorAction SilentlyContinue)
    foreach ($d in $disks) {
      $sumR += [double]$d.DiskReadBytesPersec
      $sumW += [double]$d.DiskWriteBytesPersec
    }
    if ($prevDiskR -ne $null) {
      $diskRead = [math]::Max(0, ($sumR - $prevDiskR) / $delta * 1000)
      $diskWrite = [math]::Max(0, ($sumW - $prevDiskW) / $delta * 1000)
    }
    $prevDiskR = $sumR
    $prevDiskW = $sumW

    $netIn = 0.0
    $netOut = 0.0
    $adapter = $null
    $bestTraffic = -1.0
    $nextNet = @{}
    foreach ($n in @(Get-CimInstance Win32_PerfRawData_Tcpip_NetworkInterface -ErrorAction SilentlyContinue)) {
      $name = [string]$n.Name
      if ($name -match 'Loopback|ISATAP|Teredo|Pseudo|Псевдо') { continue }
      $key = $name
      $recv = [double]$n.BytesReceivedPersec
      $sent = [double]$n.BytesSentPersec
      $nextNet[$key] = @{ r = $recv; s = $sent }
      $inRate = 0.0
      $outRate = 0.0
      if ($prevNet.ContainsKey($key)) {
        $inRate = [math]::Max(0, ($recv - $prevNet[$key].r) / $delta * 1000)
        $outRate = [math]::Max(0, ($sent - $prevNet[$key].s) / $delta * 1000)
      }
      $netIn += $inRate
      $netOut += $outRate
      if (($inRate + $outRate) -gt $bestTraffic) {
        $bestTraffic = $inRate + $outRate
        $adapter = $name
      }
    }
    $prevNet = $nextNet
    $prevMs = $nowMs

    $ramTotal = [double]$os.TotalVisibleMemorySize * 1024
    $ramFree = [double]$os.FreePhysicalMemory * 1024

    $payload = [ordered]@{
      t = [double]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
      cpu = [double]$total
      cores = @($cores)
      ramUsed = [double]($ramTotal - $ramFree)
      ramTotal = [double]$ramTotal
      diskRead = [double]$diskRead
      diskWrite = [double]$diskWrite
      netIn = [double]$netIn
      netOut = [double]$netOut
      netAdapter = $adapter
    }
    Write-Output ($payload | ConvertTo-Json -Compress -Depth 4)
  } catch {
    Write-Output ('{"error":' + (ConvertTo-Json -Compress ([string]$_.Exception.Message)) + '}')
  }
  Start-Sleep -Milliseconds 1000
}
