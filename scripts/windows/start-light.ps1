param([string]$AppDirectory = $PSScriptRoot)
$ErrorActionPreference = 'Stop'
$exe = Join-Path $AppDirectory 'bezel.exe'
$stateFile = Join-Path $AppDirectory 'light-process.json'
$log = Join-Path $AppDirectory 'light-startup.log'
$mutex = New-Object Threading.Mutex($false, 'Local\BezelLightStartup')
if (-not $mutex.WaitOne(0)) { $mutex.Dispose(); exit }
try {
    if (Get-Process bezel-studio -ErrorAction SilentlyContinue) {
        throw 'Close Bezel Studio before starting the light runtime.'
    }
    if (Test-Path -LiteralPath $stateFile) {
        $state = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
        $running = Get-Process -Id $state.pid -ErrorAction SilentlyContinue
        if ($running -and $running.Path -eq $exe -and
            $running.StartTime.ToUniversalTime().ToString('o') -eq $state.started) { exit }
    }
    $settingsFile = Join-Path $env:APPDATA 'io.github.slipalison.bezel\settings.json'
    $settings = Get-Content -LiteralPath $settingsFile -Raw | ConvertFrom-Json
    if (-not $settings.lastTheme -or -not (Test-Path -LiteralPath $settings.lastTheme)) {
        throw 'Save a theme in Studio before starting the light runtime.'
    }
    if (-not $settings.liveScreen) { throw 'Select a screen in Studio first.' }
    # Wake the configured embedded reader. Only its task holds administrator rights.
    $snapshot = Join-Path $env:LOCALAPPDATA 'io.github.slipalison.bezel\sensors\hardware.json'
    $heartbeat = Join-Path (Split-Path -Parent $snapshot) 'request'
    $embedded = Test-Path -LiteralPath (Join-Path $AppDirectory 'sensors\bezel-sensors-helper.exe')
    function Request-Sensors {
        if ($embedded) {
            New-Item -ItemType Directory -Path (Split-Path -Parent $heartbeat) -Force | Out-Null
            $file = [IO.File]::Open($heartbeat, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
            $file.Dispose()
            [IO.File]::SetLastWriteTimeUtc($heartbeat, [DateTime]::UtcNow)
        }
    }
    Request-Sensors
    if (Test-Path -LiteralPath (Join-Path $AppDirectory 'sensors\bezel-sensors-helper.exe')) {
        $task = Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -ErrorAction SilentlyContinue
        if ($task) {
            try { Start-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' }
            catch { Add-Content -LiteralPath $log -Value "$(Get-Date -Format o) Sensor task could not start: $_" }
        }
    }
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        Request-Sensors
        if ((Test-Path -LiteralPath $snapshot) -and
            [DateTime]::UtcNow - (Get-Item -LiteralPath $snapshot).LastWriteTimeUtc -lt [TimeSpan]::FromSeconds(6)) { break }
        try {
            $request = [Net.HttpWebRequest]::Create('http://127.0.0.1:8085/data.json')
            $request.Proxy = $null
            $request.Timeout = 1000
            $request.AllowAutoRedirect = $false
            $response = $request.GetResponse()
            $response.Close()
            break
        } catch { Start-Sleep -Seconds 2 }
    }
    $arguments = 'run "{0}" --screen "{1}" --screens-config "{2}" --tray' -f $settings.lastTheme, $settings.liveScreen, $settingsFile
    foreach ($option in @(@('ffmpegPath','--ffmpeg'), @('pingHost','--ping-host'), @('mangohudDir','--mangohud-dir'))) {
        $value = $settings.($option[0])
        if ($value) { $arguments += ' {0} "{1}"' -f $option[1], $value }
    }
    for ($attempt = 1; $attempt -le 6; $attempt++) {
        $process = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $AppDirectory -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $AppDirectory 'light-output.log') -RedirectStandardError (Join-Path $AppDirectory 'light-error.log')
        $started = $process.StartTime.ToUniversalTime().ToString('o')
        Start-Sleep -Seconds 5
        if (-not $process.HasExited) {
            @{ pid = $process.Id; started = $started; executable = $exe; theme = $settings.lastTheme; screen = $settings.liveScreen } | ConvertTo-Json | Set-Content -LiteralPath $stateFile -Encoding UTF8
            Add-Content -LiteralPath $log -Value "$(Get-Date -Format o) Started PID $($process.Id): $arguments"
            exit
        }
        Add-Content -LiteralPath $log -Value "$(Get-Date -Format o) Attempt $attempt failed: exit $($process.ExitCode)"
        Start-Sleep -Seconds 3
    }
    throw 'Light runtime could not start. See light-error.log.'
} catch {
    Add-Content -LiteralPath $log -Value "$(Get-Date -Format o) ERROR: $_"
    Write-Error $_
} finally {
    $mutex.ReleaseMutex()
    $mutex.Dispose()
}
