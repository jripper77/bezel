param([string]$Version, [switch]$SkipBuild, [string]$LibreDirectory)
$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
Push-Location $repository
try {
    if (-not $SkipBuild) {
        & (Join-Path $PSScriptRoot 'build-evo.ps1') -Version $Version
    }
    $releaseVersion = [IO.File]::ReadAllText((Join-Path $repository 'VERSION')).Trim()
    if ($Version -and $Version -ne $releaseVersion) { throw 'Requested version differs from VERSION.' }
    $cli = Join-Path $repository 'target\release\bezel.exe'
    if ((& $cli --version) -ne "bezel $releaseVersion") { throw 'Build both binaries for VERSION before packaging.' }
    $studioInfo = (Get-Item -LiteralPath (Join-Path $repository 'target\release\bezel-studio.exe')).VersionInfo
    if ($studioInfo.ProductVersion -ne $releaseVersion -or $studioInfo.ProductName -ne 'Bezel Evo') { throw 'Studio Windows metadata differs from VERSION. Rebuild before packaging.' }
    if (-not $LibreDirectory) {
        $archive = Join-Path $repository 'target\LibreHardwareMonitor-0.9.6.zip'
        $digest = '086d9f1b5a99e643edc2cfaaac16051685b551e4c5ac0b32a57c58c0e529c001'
        if (-not (Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive).Hash -ine $digest) {
            Invoke-WebRequest 'https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases/download/v0.9.6/LibreHardwareMonitor.zip' -UseBasicParsing -OutFile $archive
        }
        if ((Get-FileHash -LiteralPath $archive).Hash -ine $digest) { throw 'LibreHardwareMonitor archive checksum mismatch.' }
        $LibreDirectory = Join-Path $repository 'target\package-libre'
        Expand-Archive -LiteralPath $archive -DestinationPath $LibreDirectory -Force
    }
    $sensors = Join-Path $repository 'target\release\sensors'
    if (Test-Path -LiteralPath $sensors) {
        $resolvedSensors = (Resolve-Path -LiteralPath $sensors).Path
        if ($resolvedSensors -ine (Join-Path $repository 'target\release\sensors')) { throw 'Unsafe sensor staging path.' }
        Remove-Item -LiteralPath $resolvedSensors -Recurse -Force
    }
    & (Join-Path $PSScriptRoot 'build-sensors.ps1') -LibreDirectory $LibreDirectory -OutputDirectory $sensors
    $selfTest = Start-Process -FilePath (Join-Path $sensors 'bezel-sensors-helper.exe') -ArgumentList '--self-test' -WindowStyle Hidden -Wait -PassThru
    if ($selfTest.ExitCode -ne 0) { throw 'Sensor helper self-test failed.' }
    $stage = Join-Path $repository 'target\windows-package'
    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    # The manifest is explicit. Never copy dist/bezel, which can hold personal
    # themes, logs, hardware labels, backup files and startup identities.
    $manifest = [ordered]@{}
    function Add-Payload([string]$Source, [string]$Name) {
        $destination = Join-Path $stage $Name
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        Copy-Item -LiteralPath $Source -Destination $destination -Force
        $manifest[$Name.Replace('\','/')] = $destination
    }
    Add-Payload $cli 'bezel.exe'
    Add-Payload (Join-Path $repository 'target\release\bezel-studio.exe') 'bezel-studio.exe'
    foreach ($name in @('start-light.ps1','start-light.cmd','open-studio.ps1','open-studio.cmd','installer-maintenance.ps1','setup-evo-sensors.cmd')) {
        Add-Payload (Join-Path $PSScriptRoot $name) $name
    }
    Add-Payload (Join-Path $repository 'LICENSE') 'LICENSE'
    Add-Payload (Join-Path $repository 'apps\bezel-studio\src\assets\tabler\LICENSE') 'LICENSE-Tabler-Icons.txt'
    Add-Payload (Join-Path $repository 'apps\bezel-studio\src\assets\mdi\LICENSE') 'LICENSE-Material-Design-Icons.txt'
    Add-Payload (Join-Path $repository 'apps\bezel-studio\src\assets\mdi\NOTICE') 'NOTICE-Material-Design-Icons.txt'
    Add-Payload (Join-Path $repository 'packaging\windows\LICENSE-Tauri-template.txt') 'LICENSE-Tauri-template.txt'
    Add-Payload (Join-Path $repository 'packaging\windows\README.md') 'README.md'
    foreach ($root in @(@('themes', (Join-Path $repository 'themes')), @('sensors', $sensors))) {
        foreach ($file in Get-ChildItem -LiteralPath $root[1] -File -Recurse) {
            $relative = $file.FullName.Substring($root[1].Length).TrimStart('\')
            Add-Payload $file.FullName (Join-Path $root[0] $relative)
        }
    }
    $checksums = @($manifest.GetEnumerator() | ForEach-Object { '{0}  {1}' -f (Get-FileHash -LiteralPath $_.Value).Hash.ToLowerInvariant(), $_.Key })
    [IO.File]::WriteAllLines((Join-Path $stage 'SHA256SUMS.txt'), $checksums, [Text.UTF8Encoding]::new($false))
    $manifest['SHA256SUMS.txt'] = Join-Path $stage 'SHA256SUMS.txt'
    $resources = [ordered]@{}
    foreach ($entry in $manifest.GetEnumerator()) {
        if ($entry.Key -ne 'bezel-studio.exe' -and -not $entry.Key.StartsWith('themes/')) { $resources[$entry.Value.Replace('\','/')] = $entry.Key }
    }
    $hooksPath = Join-Path $repository 'target\windows-hooks.nsh'
    $hooks = [IO.File]::ReadAllText((Join-Path $repository 'packaging\windows\hooks.nsh')).Replace('@EVO_MAINTENANCE@', (Join-Path $PSScriptRoot 'installer-maintenance.ps1'))
    [IO.File]::WriteAllText($hooksPath, $hooks, [Text.UTF8Encoding]::new($false))
    $config = @{
        version = $releaseVersion
        bundle = @{
            targets = @('nsis')
            licenseFile = (Join-Path $repository 'LICENSE').Replace('\','/')
            resources = $resources
            windows = @{
                allowDowngrades = $false
                webviewInstallMode = @{ type = 'downloadBootstrapper'; silent = $true }
                nsis = @{
                    installMode = 'currentUser'
                    languages = @('English','Italian')
                    displayLanguageSelector = $true
                    startMenuFolder = 'Bezel Evo'
                    template = (Join-Path $repository 'packaging\windows\installer.nsi').Replace('\','/')
                    installerHooks = $hooksPath.Replace('\','/')
                }
            }
        }
    }
    $configPath = Join-Path $repository 'target\windows-bundle.json'
    [IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
    $tauriManifest = Join-Path $repository 'apps\bezel-studio\src-tauri\Cargo.toml'
    $manifestBytes = [IO.File]::ReadAllBytes($tauriManifest)
    Push-Location (Join-Path $repository 'apps\bezel-studio')
    try {
        $ErrorActionPreference = 'Continue'
        & npx --yes '@tauri-apps/cli@2.12.1' bundle --bundles nsis --config $configPath --ci --no-binary-patching
        $bundleExit = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if ($bundleExit -ne 0) { throw 'Tauri NSIS packaging failed.' }
    } finally { [IO.File]::WriteAllBytes($tauriManifest, $manifestBytes); Pop-Location }
    $output = Join-Path $repository 'dist'
    New-Item -ItemType Directory -Path $output -Force | Out-Null
    $setup = Get-ChildItem -LiteralPath (Join-Path $repository 'target\release\bundle\nsis') -Filter "*${releaseVersion}*setup.exe" | Select-Object -First 1
    if (-not $setup) { throw 'Installer output missing.' }
    $setupName = "Bezel-Evo-$releaseVersion-windows-x64-setup.exe"
    Copy-Item -LiteralPath $setup.FullName -Destination (Join-Path $output $setupName) -Force
    # Compress precisely the manifest, preserving relative paths.
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipPath = Join-Path $output "Bezel-Evo-$releaseVersion-windows-x64-portable.zip"
    $zipFile = [IO.File]::Open($zipPath, [IO.FileMode]::Create)
    $zip = New-Object IO.Compression.ZipArchive($zipFile, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($entry in $manifest.GetEnumerator()) { [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $entry.Value, $entry.Key) | Out-Null }
    } finally { $zip.Dispose(); $zipFile.Dispose() }
    $releaseFiles = @((Join-Path $output $setupName), $zipPath)
    $sums = @($releaseFiles | ForEach-Object { '{0}  {1}' -f (Get-FileHash -LiteralPath $_).Hash.ToLowerInvariant(), [IO.Path]::GetFileName($_) })
    [IO.File]::WriteAllLines((Join-Path $output "Bezel-Evo-$releaseVersion-SHA256SUMS.txt"), $sums, [Text.UTF8Encoding]::new($false))
    Write-Output "Ready: $setupName and $([IO.Path]::GetFileName($zipPath))"
} finally { Pop-Location }
