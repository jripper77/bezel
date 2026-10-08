param([string]$Version)
$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
if (-not $Version) { $Version = [IO.File]::ReadAllText((Join-Path $repository 'VERSION')).Trim() }
$setup = Join-Path $repository "dist\Bezel-Evo-$Version-windows-x64-setup.exe"
$directory = Join-Path $repository 'target\installer-smoke'
$registry = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Bezel Evo'
$manufacturer = 'HKCU:\Software\jripper77\Bezel Evo'
$menu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\Bezel Evo'
$desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Bezel Evo.lnk'
foreach ($path in @($directory,$registry,$manufacturer,$menu,$desktop)) {
    if (Test-Path -LiteralPath $path) { throw "Smoke test needs an unused installation location and namespace: $path" }
}
$taskBefore = Export-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -ErrorAction SilentlyContinue
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
function Snapshot-Run {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Run')
    if (-not $key) { return '[]' }
    try {
        $entries = @($key.GetValueNames() | Sort-Object | ForEach-Object {
            [pscustomobject]@{ name=$_; kind=$key.GetValueKind($_).ToString(); data=$key.GetValue($_,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
        })
        return ConvertTo-Json -InputObject $entries -Depth 6
    } finally { $key.Dispose() }
}
$startupBefore = Snapshot-Run
[IO.File]::WriteAllText((Join-Path $repository 'target\startup-run-before-smoke.json'), $startupBefore)
$dataDirectory = Join-Path $env:APPDATA 'io.github.slipalison.bezel'
New-Item -ItemType Directory -Path $dataDirectory -Force | Out-Null
$marker = Join-Path $dataDirectory ('installer-preservation-' + [Guid]::NewGuid().ToString('N') + '.txt')
[IO.File]::WriteAllText($marker, 'User data must survive uninstall.')
function Run-Setup {
    $process = Start-Process -FilePath $setup -ArgumentList "/S /D=$directory" -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
}
function Assert-Payload {
    foreach ($line in Get-Content -LiteralPath (Join-Path $directory 'SHA256SUMS.txt')) {
        $parts = $line -split '  ',2
        if ((Get-FileHash -LiteralPath (Join-Path $directory $parts[1])).Hash -ine $parts[0]) { throw "Payload checksum failed: $($parts[1])" }
    }
    $registered = Get-ItemProperty -LiteralPath $registry
    if ($registered.DisplayVersion -ne $Version -or $registered.Publisher -ne 'jripper77') { throw 'Installed release metadata mismatch.' }
    if ((& (Join-Path $directory 'bezel.exe') --version) -ne "bezel $Version") { throw 'Installed CLI version mismatch.' }
}
try {
    Run-Setup
    Assert-Payload
    $maintenance = Join-Path $directory 'installer-maintenance.ps1'
    $priorLight = (Get-ItemProperty -LiteralPath $runKey -Name BezelEvoLight -ErrorAction SilentlyContinue).BezelEvoLight
    if (-not $priorLight) {
        & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $maintenance -Mode Startup -AppDirectory $directory -Startup 1
        if ($LASTEXITCODE -ne 0) { throw 'Optional Light setup failed.' }
        $entry = (Get-ItemProperty -LiteralPath $runKey -Name BezelEvoLight).BezelEvoLight
        if (-not $entry.Contains('"' + (Join-Path $directory 'start-light.ps1') + '"')) { throw 'Optional Light setup points to a different installation.' }
        & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $maintenance -Mode Startup -AppDirectory $directory -Startup 0
        if ($LASTEXITCODE -ne 0 -or (Get-ItemProperty -LiteralPath $runKey -Name BezelEvoLight -ErrorAction SilentlyContinue).BezelEvoLight) { throw 'Optional Light removal failed.' }
    }
    if ($taskBefore) {
        # An existing reader elsewhere must never be retargeted by Setup.
        $task = Get-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\'
        if ($task.Actions[0].Execute -ine (Join-Path $directory 'sensors\bezel-sensors-helper.exe')) {
            & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $maintenance -Mode Sensors -AppDirectory $directory | Out-Null
            if ($LASTEXITCODE -eq 0) { throw 'Sensor setup accepted another installation''s task.' }
        }
    }
    $theme = Join-Path $directory 'my-personal-theme.bezeltheme'
    [IO.File]::WriteAllText($theme, 'User theme sentinel.')
    Run-Setup
    Assert-Payload
    if ([IO.File]::ReadAllText($theme) -ne 'User theme sentinel.') { throw 'Upgrade changed a personal file.' }
    $process = Start-Process -FilePath (Join-Path $directory 'uninstall.exe') -ArgumentList "/S _?=$directory" -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "Uninstaller failed: $($process.ExitCode)" }
    if (Test-Path -LiteralPath (Join-Path $directory 'bezel-studio.exe')) { throw 'Uninstall left Studio installed.' }
    if (Test-Path -LiteralPath $registry) { throw 'Uninstall left its application registration.' }
    if (-not (Test-Path -LiteralPath $theme) -or -not (Test-Path -LiteralPath $marker)) { throw 'Uninstall deleted a personal file.' }
    $taskAfter = Export-ScheduledTask -TaskName 'Bezel-Sensors' -TaskPath '\' -ErrorAction SilentlyContinue
    $startupAfter = Snapshot-Run
    if ($taskBefore -ne $taskAfter -or $startupBefore -ne $startupAfter) { throw 'Silent install changed existing optional services.' }
    # Only our known fixture files are removed; never recursively delete data.
    Remove-Item -LiteralPath $theme
    if (Test-Path -LiteralPath (Join-Path $directory 'uninstall.exe')) { Remove-Item -LiteralPath (Join-Path $directory 'uninstall.exe') }
    Remove-Item -LiteralPath $directory
    Write-Output 'PASS: install, upgrade, all payload hashes, user-data preservation, optional-service isolation and uninstall.'
} finally {
    if (Test-Path -LiteralPath $marker) { Remove-Item -LiteralPath $marker }
}
