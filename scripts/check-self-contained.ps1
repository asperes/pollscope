# Fails if the given executable imports the VC++ or UCRT redistributable DLLs.
param([Parameter(Mandatory)][string]$Exe)
$ErrorActionPreference = 'Stop'
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -property installationPath
$dumpbin = Get-ChildItem "$vs\VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe" | Select-Object -First 1
if (-not $dumpbin) { throw "dumpbin.exe not found under $vs" }
$deps = & $dumpbin.FullName /nologo /dependents $Exe
$deps
$bad = $deps | Select-String -Pattern '(?i)^\s*(vcruntime\d+|msvcp\d+|api-ms-win-crt-[a-z0-9-]+|ucrtbase)\.dll'
if ($bad) {
  Write-Error "Executable depends on redistributable runtime DLLs:`n$($bad -join "`n")"
  exit 1
}
Write-Host "OK: $Exe needs no VC++ or UCRT redistributable."
