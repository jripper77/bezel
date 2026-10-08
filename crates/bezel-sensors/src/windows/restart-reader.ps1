$ErrorActionPreference = 'Stop'
$folder = Join-Path $env:LOCALAPPDATA 'io.github.slipalison.bezel\sensors'
$snapshot = Join-Path $folder 'hardware.json'
$heartbeat = Join-Path $folder 'request'
$requestedAt = [DateTime]::UtcNow
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$mutex = New-Object Threading.Mutex($false, ('Local\BezelSensorsRestart-' + $sid))
$owned = $false
function Request-Reader {
    New-Item -ItemType Directory -Path $folder -Force | Out-Null
    $file = [IO.File]::Open($heartbeat, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
    $file.Dispose()
    [IO.File]::SetLastWriteTimeUtc($heartbeat, [DateTime]::UtcNow)
}
function Fresh-Snapshot {
    if (-not (Test-Path -LiteralPath $snapshot)) { return $false }
    try {
        $modified = (Get-Item -LiteralPath $snapshot).LastWriteTimeUtc
        if ($modified -lt $requestedAt -or [DateTime]::UtcNow - $modified -gt [TimeSpan]::FromSeconds(6)) { return $false }
        $data = Get-Content -LiteralPath $snapshot -Raw | ConvertFrom-Json
        return $data.Provider -eq 'Bezel LibreHardwareMonitorLib' -and $data.Elevated -and $data.Children.Count -gt 0
    } catch { return $false }
}
try {
    try { $owned = $mutex.WaitOne(75000) } catch [Threading.AbandonedMutexException] { $owned = $true }
    if (-not $owned) { throw 'Another sensor recovery did not finish in time' }
    # Another Bezel client may already have completed this same recovery.
    if (Fresh-Snapshot) { Write-Output 'Fresh reader snapshot from concurrent recovery'; exit 0 }
    Request-Reader
    Write-Output ('before: state=' + (Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\').State)
    Stop-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\'
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ((Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\').State -eq 'Running') {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Sensor reader did not stop in time' }
        Start-Sleep -Milliseconds 200
    }
    Start-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\'
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    $retryAt = [DateTime]::UtcNow.AddSeconds(5)
    $attempts = 1
    do {
        Request-Reader
        if (Fresh-Snapshot) { Write-Output ('Reader ready: fresh snapshot; starts=' + $attempts); exit 0 }
        $state = (Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\').State
        if ($state -ne 'Running' -and [DateTime]::UtcNow -ge $retryAt -and $attempts -lt 3) {
            $info = Get-ScheduledTaskInfo -TaskName 'Bezel-Sensors' -TaskPath '\'
            Write-Output ('Reader exited before publishing; result=' + $info.LastTaskResult + '; retrying')
            Start-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\'
            $attempts++
            $retryAt = [DateTime]::UtcNow.AddSeconds(5)
        }
        Start-Sleep -Milliseconds 500
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Sensor task did not publish a fresh snapshot within 60 seconds; see sensors/helper.log'
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
} finally {
    if ($owned) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
