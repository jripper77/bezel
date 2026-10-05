param(
    [string]$AppDirectory = $PSScriptRoot,
    [string]$LibreDirectory = 'F:\dev\tools\LibreHardwareMonitor-0.9.6'
)
$ErrorActionPreference = 'Stop'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this script in PowerShell as administrator, using your normal Windows account.'
}
$libreExe = Join-Path $LibreDirectory 'LibreHardwareMonitor.exe'
$integrated = Test-Path -LiteralPath (Join-Path $AppDirectory 'sensors\bezel-sensors-helper.exe')
$required = @((Join-Path $AppDirectory 'bezel.exe'), (Join-Path $AppDirectory 'start-light.ps1'))
if (-not $integrated) { $required += $libreExe }
foreach ($file in $required) {
    if (-not (Test-Path -LiteralPath $file)) { throw "Missing file: $file" }
}
$backup = Join-Path $AppDirectory 'startup-backup'
New-Item -ItemType Directory -Path $backup -Force | Out-Null
if (-not $integrated) {
$configFile = Join-Path $LibreDirectory 'LibreHardwareMonitor.config'
[xml]$config = Get-Content -LiteralPath $configFile -Raw
foreach ($name in @('startMinMenuItem', 'runWebServerMenuItem')) {
    $node = $config.SelectSingleNode("//add[@key='$name']")
    if (-not $node -or $node.value -ne 'true') { throw "Enable $name in LibreHardwareMonitor before configuring startup." }
}
$port = $config.SelectSingleNode("//add[@key='listenerPort']")
if ($port -and $port.value -ne '8085') { throw 'LibreHardwareMonitor must use port 8085.' }
}
# Only these three legacy Turing tasks are removed. Their programs are left installed.
foreach ($name in @('TempMonitor_0_3', 'TempMonitor_8', 'UsbMonitor')) {
    $task = Get-ScheduledTask -TaskName $name -TaskPath '\' -ErrorAction SilentlyContinue
    if ($task) {
        $xml = Join-Path $backup "$name.xml"
        if (-not (Test-Path -LiteralPath $xml)) { Export-ScheduledTask -TaskName $name -TaskPath '\' | Set-Content -LiteralPath $xml -Encoding Unicode }
        Unregister-ScheduledTask -TaskName $name -TaskPath '\' -Confirm:$false
        Write-Host "Removed legacy startup task: $name"
    }
}
if ($integrated) {
    & (Join-Path $AppDirectory 'configure-sensors.ps1') -AppDirectory $AppDirectory
} else {
$action = New-ScheduledTaskAction -Execute $libreExe -WorkingDirectory $LibreDirectory
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity.Name
$taskPrincipal = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Highest
$taskSettings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName 'Bezel-LibreHardwareMonitor' -TaskPath '\' -Action $action -Trigger $trigger -Principal $taskPrincipal -Settings $taskSettings -Description 'Start installed LibreHardwareMonitor minimized for Bezel sensors.' -Force | Out-Null
}
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$old = (Get-ItemProperty -Path $runKey -Name Bezel -ErrorAction SilentlyContinue).Bezel
$oldFile = Join-Path $backup 'bezel-run.txt'
if (-not (Test-Path -LiteralPath $oldFile)) { [string]$old | Set-Content -LiteralPath $oldFile -Encoding UTF8 }
$powershell = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
$command = '"{0}" -NoProfile -WindowStyle Hidden -File "{1}"' -f $powershell, (Join-Path $AppDirectory 'start-light.ps1')
Set-ItemProperty -Path $runKey -Name Bezel -Value $command
Write-Host 'Configured light Bezel runtime and its elevated hardware sensor reader at login.'
Write-Host 'Close Studio, then run start-light.ps1 to test without rebooting.'
