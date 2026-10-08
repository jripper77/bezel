param([string]$Version)
$ErrorActionPreference = 'Stop'
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$versionFile = Join-Path $repository 'VERSION'
$current = [IO.File]::ReadAllText($versionFile).Trim()
if (-not $Version) {
    if ($current -notmatch '^(\d+)\.(\d+)\.(\d+)$') { throw "Invalid VERSION: $current" }
    $Version = '{0}.{1}.{2}' -f $Matches[1], $Matches[2], ([int]$Matches[3] + 1)
}
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Use a version such as 0.1.1.' }
[IO.File]::WriteAllText($versionFile, "$Version`n", [Text.UTF8Encoding]::new($false))
Write-Output "Building Bezel Evo $Version"
$previousStamp = $env:BEZEL_VERSION
$previousTauriConfig = $env:TAURI_CONFIG
try {
    $env:BEZEL_VERSION = $Version
    $tauriOverlay = if ($previousTauriConfig) { $previousTauriConfig | ConvertFrom-Json } else { [pscustomobject]@{} }
    $tauriOverlay | Add-Member -NotePropertyName version -NotePropertyValue $Version -Force
    $env:TAURI_CONFIG = $tauriOverlay | ConvertTo-Json -Depth 12 -Compress
    Push-Location $repository
    try {
        # Windows PowerShell represents normal Cargo stderr progress as error
        # records when output is redirected. The exit code decides success.
        $ErrorActionPreference = 'Continue'
        & cargo build --release -p bezel -p bezel-studio --offline
        $buildExit = $LASTEXITCODE
        $ErrorActionPreference = 'Stop'
        if ($buildExit -ne 0) { throw "Build failed for $Version; rerun with -Version $Version." }
        & (Join-Path $repository 'target\release\bezel.exe') --version
        if ($LASTEXITCODE -ne 0) { throw 'Could not verify CLI version.' }
    } finally { Pop-Location }
} finally { $env:BEZEL_VERSION = $previousStamp; $env:TAURI_CONFIG = $previousTauriConfig }
Write-Output "Ready: target\release\bezel.exe and bezel-studio.exe ($Version)."
