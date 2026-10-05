param([string]$AppDirectory = $PSScriptRoot)
$ErrorActionPreference = 'Stop'
$stateFile = Join-Path $AppDirectory 'light-process.json'
if (-not (Test-Path -LiteralPath $stateFile)) { exit }
$state = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
$process = Get-Process -Id $state.pid -ErrorAction SilentlyContinue
# Never stop a reused PID or another installation.
if ($process -and $process.Path -eq (Join-Path $AppDirectory 'bezel.exe') -and
    $process.StartTime.ToUniversalTime().ToString('o') -eq $state.started) {
    Stop-Process -Id $process.Id
    $process.WaitForExit(5000) | Out-Null
}
Remove-Item -LiteralPath $stateFile
