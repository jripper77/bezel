param(
    [Parameter(Mandatory=$true)][ValidateSet('Check','Startup','Sensors','Uninstall')][string]$Mode,
    [Parameter(Mandatory=$true)][string]$AppDirectory,
    [ValidateSet('0','1')][string]$Startup = '0',
    [string]$ExpectedUserSid,
    [switch]$Elevated
)
$ErrorActionPreference = 'Stop'
try {
    $appPath = [IO.Path]::GetFullPath($AppDirectory).TrimEnd('\')
    $helper = Join-Path $appPath 'sensors\bezel-sensors-helper.exe'
    $runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if ($ExpectedUserSid -and $identity.User.Value -ne $ExpectedUserSid) {
        throw 'Use the same Windows account for sensor setup and installation.'
    }
    function Owned-Task {
        $task = Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -ErrorAction SilentlyContinue
        if ($task -and @($task.Actions).Count -eq 1 -and
            $task.Principal.UserId -in @($identity.Name, $identity.User.Value) -and
            [IO.Path]::GetFullPath($task.Actions[0].Execute).TrimEnd('\') -ieq $helper) { return $task }
        return $null
    }
    function Elevate-Maintenance {
        $shell = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
        $arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -Mode {1} -AppDirectory "{2}" -ExpectedUserSid "{3}" -Elevated' -f $PSCommandPath, $Mode, $appPath, $identity.User.Value
        $process = Start-Process -FilePath $shell -ArgumentList $arguments -Verb RunAs -WindowStyle Hidden -Wait -PassThru
        if ($process.ExitCode -ne 0) { throw 'Sensor task setup/removal failed or administrator approval was cancelled. See setup-sensors.log.' }
    }
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    $admin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    switch ($Mode) {
        'Check' {
            # Never kill Studio: an editor can have unsaved documents. The helper
            # exits after its clients close; wait briefly for its DLL handles.
            $names = @('bezel.exe','bezel-studio.exe','bezel-sensors-helper.exe')
            $running = @(Get-CimInstance Win32_Process | Where-Object {
                $_.Name -in $names -and $_.ExecutablePath -and
                [IO.Path]::GetDirectoryName($_.ExecutablePath) -in @($appPath, (Join-Path $appPath 'sensors'))
            })
            if ($running) { throw 'Save and exit Bezel Studio, quit Bezel Light from its tray icon, then retry. The sensor helper stops within 30 seconds after its clients exit.' }
        }
        'Startup' {
            $launcher = Join-Path $appPath 'start-light.ps1'
            if (-not (Test-Path -LiteralPath $launcher)) { throw 'Missing Light launcher.' }
            $old = (Get-ItemProperty -LiteralPath $runKey -Name BezelEvoLight -ErrorAction SilentlyContinue).BezelEvoLight
            if ($Startup -eq '1') {
                if ($old -and -not $old.Contains('"' + $launcher + '"')) { throw 'Another installation owns the Light login entry. It was left unchanged.' }
                if (-not (Test-Path -LiteralPath $runKey)) { New-Item -Path $runKey | Out-Null }
                $shell = Join-Path $env:WINDIR 'System32\WindowsPowerShell\v1.0\powershell.exe'
                $command = '"{0}" -NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "{1}"' -f $shell, $launcher
                Set-ItemProperty -LiteralPath $runKey -Name BezelEvoLight -Value $command
            } elseif ($old -and $old.Contains('"' + $launcher + '"')) {
                Remove-ItemProperty -LiteralPath $runKey -Name BezelEvoLight
            }
        }
        'Sensors' {
            if (-not (Test-Path -LiteralPath $helper)) { throw 'Missing sensor helper.' }
            $task = Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -ErrorAction SilentlyContinue
            if ($task -and -not (Owned-Task)) { throw 'Another Bezel installation owns Bezel-Sensors. Its task was left unchanged. Remove or migrate that installation before setting up this reader.' }
            if (-not $admin) { Elevate-Maintenance; break }
            # Unlike the older personal migration script, do not remove vendor
            # tasks, disable external Libre or terminate another application's GUI.
            $data = Join-Path $env:LOCALAPPDATA 'io.github.slipalison.bezel\sensors'
            New-Item -ItemType Directory -Path $data -Force | Out-Null
            Set-Content -LiteralPath (Join-Path $data 'request') -Value ''
            $action = New-ScheduledTaskAction -Execute $helper -WorkingDirectory (Split-Path -Parent $helper)
            $trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity.Name
            $user = New-ScheduledTaskPrincipal -UserId $identity.Name -LogonType Interactive -RunLevel Highest
            $options = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -StartWhenAvailable -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
            Register-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -Action $action -Trigger $trigger -Principal $user -Settings $options -Description 'Bezel Evo hardware sensor reader for the current user.' -Force | Out-Null
            Start-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\'
            Add-Content -LiteralPath (Join-Path $appPath 'setup-sensors.log') -Value 'Bezel Evo sensor task configured. Check reader status in Studio.'
        }
        'Uninstall' {
            $task = Owned-Task
            if ($task) {
                if (-not $admin) { Elevate-Maintenance; break }
                Stop-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -ErrorAction SilentlyContinue
                Unregister-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -Confirm:$false
            }
            $old = (Get-ItemProperty -LiteralPath $runKey -Name BezelEvoLight -ErrorAction SilentlyContinue).BezelEvoLight
            if ($old -and $old.Contains('"' + (Join-Path $appPath 'start-light.ps1') + '"')) {
                Remove-ItemProperty -LiteralPath $runKey -Name BezelEvoLight
            }
            $studioLogin = (Get-ItemProperty -LiteralPath $runKey -Name 'Bezel Evo' -ErrorAction SilentlyContinue).'Bezel Evo'
            if ($studioLogin -and $studioLogin.Contains('"' + (Join-Path $appPath 'bezel-studio.exe') + '"')) {
                Remove-ItemProperty -LiteralPath $runKey -Name 'Bezel Evo'
            }
            $menu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Bezel Evo'
            $shell = New-Object -ComObject WScript.Shell
            foreach ($name in @('Bezel Evo Light.lnk','Setup sensors.lnk')) {
                $path = Join-Path $menu $name
                if (Test-Path -LiteralPath $path) {
                    $shortcut = $shell.CreateShortcut($path)
                    if ($shortcut.Arguments.Contains('"' + $appPath + '\')) { Remove-Item -LiteralPath $path }
                }
            }
            # Saved themes, Studio preferences and sensor snapshots are retained.
        }
    }
    exit 0
} catch {
    Write-Output $_.Exception.Message
    exit 1
}
