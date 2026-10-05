param([string]$AppDirectory = $PSScriptRoot)
$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
$log = Join-Path $AppDirectory 'setup-sensors.log'
try {
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw 'Run this setup as administrator using your normal Windows account.'
    }
    $helperDirectory = Join-Path $AppDirectory 'sensors'
    $helper = Join-Path $helperDirectory 'bezel-sensors-helper.exe'
    if (-not (Test-Path -LiteralPath $helper)) { throw "Missing bundled helper: $helper" }
    $backup = Join-Path $AppDirectory 'startup-backup'
    New-Item -ItemType Directory -Path $backup -Force | Out-Null
    $oldTask = Get-ScheduledTask -TaskName 'Bezel-LibreHardwareMonitor' -TaskPath '\' -ErrorAction SilentlyContinue
    if ($oldTask -and -not (Test-Path (Join-Path $backup 'Bezel-LibreHardwareMonitor.xml'))) {
        Export-ScheduledTask -TaskName 'Bezel-LibreHardwareMonitor' -TaskPath '\' | Set-Content -LiteralPath (Join-Path $backup 'Bezel-LibreHardwareMonitor.xml') -Encoding Unicode
    }
    $data = Join-Path $env:LOCALAPPDATA 'io.github.slipalison.bezel\sensors'
    New-Item -ItemType Directory -Path $data -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $data 'request') -Value ''
    $action = New-ScheduledTaskAction -Execute $helper -WorkingDirectory $helperDirectory
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity.Name
    $taskPrincipal = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Highest
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    Register-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -Action $action -Trigger $trigger -Principal $taskPrincipal -Settings $settings -Description 'Bezel headless LibreHardwareMonitorLib sensor reader; exits when no clients remain.' -Force | Out-Null
    Start-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\'
    $snapshot = Join-Path $data 'hardware.json'
    $ready = $false
    function Find-CpuTemperature($node) {
        if ($node.SensorId -match '^/(intelcpu|amdcpu)/' -and $node.Type -eq 'Temperature' -and $null -ne $node.RawValue) { return $true }
        foreach ($child in $node.Children) { if (Find-CpuTemperature $child) { return $true } }
        return $false
    }
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        (Get-Item (Join-Path $data 'request')).LastWriteTimeUtc = [DateTime]::UtcNow
        if (Test-Path -LiteralPath $snapshot) {
            try {
                $value = Get-Content -LiteralPath $snapshot -Raw | ConvertFrom-Json
                if ($value.Provider -eq 'Bezel LibreHardwareMonitorLib' -and $value.Elevated -and
                    [DateTime]::UtcNow - (Get-Item $snapshot).LastWriteTimeUtc -lt [TimeSpan]::FromSeconds(6) -and
                    (Find-CpuTemperature $value)) { $ready = $true; break }
            } catch { }
        }
        Start-Sleep -Seconds 1
    }
    if (-not $ready) { throw 'The bundled helper did not produce CPU temperatures. External Libre startup was left enabled; see the per-user sensors/helper.log.' }
    # Replace the existing GUI helper only after the bundled reader is working.
    if ($oldTask) {
        Disable-ScheduledTask -TaskName 'Bezel-LibreHardwareMonitor' -TaskPath '\' | Out-Null
        foreach ($oldAction in $oldTask.Actions) {
            if ([IO.Path]::GetFileName($oldAction.Execute) -ieq 'LibreHardwareMonitor.exe') {
                Get-CimInstance Win32_Process -Filter "Name='LibreHardwareMonitor.exe'" |
                    Where-Object ExecutablePath -eq $oldAction.Execute |
                    ForEach-Object { Stop-Process -Id $_.ProcessId }
            }
        }
    }
    Add-Content -LiteralPath $log -Value "$(Get-Date -Format o) SUCCESS: integrated sensor helper is elevated and reads CPU temperatures; external Libre startup disabled."
    Write-Host 'Bezel sensors integrated successfully. Libre GUI is no longer needed.'
} catch {
    Add-Content -LiteralPath $log -Value "$(Get-Date -Format o) ERROR: $_"
    throw
}
