param([string]$AppDirectory = $PSScriptRoot)
# Studio now asks the tray runtime to yield, and light resumes when Studio fully exits.
Start-Process -FilePath (Join-Path $AppDirectory 'bezel-studio.exe') -WorkingDirectory $AppDirectory
