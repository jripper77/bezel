param(
    [string]$LibreDirectory = 'F:\dev\tools\LibreHardwareMonitor-0.9.6',
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\..\target\release\sensors'),
    [switch]$ImportSensorNames
)
$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot '..\..\apps\bezel-sensors-helper'
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$library = Join-Path $LibreDirectory 'LibreHardwareMonitorLib.dll'
if ([Reflection.AssemblyName]::GetAssemblyName($library).Version.ToString() -ne '0.9.6.0') {
    throw 'This helper payload is pinned to LibreHardwareMonitor 0.9.6.'
}
& $compiler /nologo /optimize+ /platform:x64 /target:winexe ("/out:" + (Join-Path $OutputDirectory 'bezel-sensors-helper.exe')) ("/reference:" + $library) /reference:System.Web.Extensions.dll ("/win32manifest:" + (Join-Path $source 'app.manifest')) (Join-Path $source 'Program.cs')
if ($LASTEXITCODE -ne 0) { throw 'Sensor helper compilation failed.' }
Copy-Item -LiteralPath (Join-Path $source 'app.config') -Destination (Join-Path $OutputDirectory 'bezel-sensors-helper.exe.config')
# Copy only the actual transitive DLL dependencies of the installed LHM library.
$seen = @()
$pending = New-Object 'System.Collections.Generic.Queue[string]'
$pending.Enqueue('LibreHardwareMonitorLib.dll')
while ($pending.Count -gt 0) {
    $name = $pending.Dequeue()
    if ($seen -contains $name) { continue }
    $path = Join-Path $LibreDirectory $name
    if (Test-Path -LiteralPath $path) {
        $seen += $name
        Copy-Item -LiteralPath $path -Destination (Join-Path $OutputDirectory $name)
        $assembly = [Reflection.Assembly]::LoadFrom($path)
        foreach ($reference in $assembly.GetReferencedAssemblies()) { $pending.Enqueue($reference.Name + '.dll') }
    }
}
$licenseBase = 'https://raw.githubusercontent.com/LibreHardwareMonitor/LibreHardwareMonitor/v0.9.6/'
Invoke-WebRequest ($licenseBase + 'LICENSE') -UseBasicParsing -OutFile (Join-Path $OutputDirectory 'LICENSE-LibreHardwareMonitor.txt')
Invoke-WebRequest ($licenseBase + 'THIRD-PARTY-NOTICES.txt') -UseBasicParsing -OutFile (Join-Path $OutputDirectory 'THIRD-PARTY-NOTICES.txt')
# Preserve custom hardware/sensor labels; no GUI or control settings are imported.
$names = New-Object Xml.XmlDocument
$root = $names.CreateElement('appSettings')
$names.AppendChild($root) | Out-Null
if ($ImportSensorNames) {
[xml]$configuration = Get-Content (Join-Path $LibreDirectory 'LibreHardwareMonitor.config') -Raw
foreach ($item in $configuration.SelectNodes('//add')) {
    if ($item.key.StartsWith('/') -and $item.key.EndsWith('/name')) {
        $root.AppendChild($names.ImportNode($item, $true)) | Out-Null
    }
}
}
$names.Save((Join-Path $OutputDirectory 'sensor-names.xml'))
$seen | Set-Content (Join-Path $OutputDirectory 'bundled-dlls.txt')
Copy-Item -LiteralPath (Join-Path $source 'README.md') -Destination (Join-Path $OutputDirectory 'README.md')
Copy-Item -Path (Join-Path $source 'licenses') -Destination $OutputDirectory -Recurse -Force
$seen | ForEach-Object {
    $file = Get-Item -LiteralPath (Join-Path $OutputDirectory $_)
    [pscustomobject]@{ file=$_.ToString(); version=$file.VersionInfo.ProductVersion; sha256=(Get-FileHash $file.FullName).Hash }
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'libraries.json') -Encoding UTF8
Write-Host 'Built headless sensor helper and bundled library dependencies.'
