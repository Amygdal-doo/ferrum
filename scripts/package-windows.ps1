# Packages a built Ferrum app for Windows:
#   scripts/package-windows.ps1 -App <id> -Version <semver> -BinaryDir <dir> -OutDir <dir>
#
# Produces, in OutDir:
#   <id>-<version>-x86_64-pc-windows-msvc.zip   exes + <id>.ico, read by Ferrum Launcher
#   <AppFileName>-<version>-x86_64-setup.exe     Inno Setup installer for people
# Pass -SkipInstaller to build only the zip (e.g. when Inno Setup is missing).
# Keep the app table in sync with scripts/lib/apps.sh.
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $App,
    [Parameter(Mandatory)] [string] $Version,
    [Parameter(Mandatory)] [string] $BinaryDir,
    [Parameter(Mandatory)] [string] $OutDir,
    [string] $Target = "x86_64-pc-windows-msvc",
    [switch] $SkipInstaller
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$apps = @{
    api     = @{ Name = "FerrumAPI";      Binaries = @("ferrumapi") }
    git     = @{ Name = "FerrumGIT";      Binaries = @("ferrumgit") }
    data    = @{ Name = "FerrumData";     Binaries = @("ferrumdata") }
    kit     = @{ Name = "FerrumKit";      Binaries = @("ferrumkit") }
    dock    = @{ Name = "FerrumContainers"; Binaries = @("ferrumdock") }
    editor  = @{ Name = "Ferrum Editor";  Binaries = @("ferrumeditor") }
    ports   = @{ Name = "FerrumPorts";    Binaries = @("ferrumports") }
    env     = @{ Name = "FerrumEnv";      Binaries = @("ferrumenv") }
    launcher = @{ Name = "Ferrum Launcher"; Binaries = @("ferrum-launcher") }
}
if ($App -eq "term") {
    throw "FerrumTerm has no Windows build yet (its server uses Unix sockets)."
}
if ($App -eq "agentic") {
    throw "FerrumAgentic has no Windows build yet (it embeds FerrumTerm, whose server uses Unix sockets)."
}
if (-not $apps.ContainsKey($App)) {
    throw "unknown app id '$App'"
}
if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$') {
    throw "'$Version' is not a semver version"
}
$numericVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3]).0"

$meta = $apps[$App]
$name = $meta.Name
$fileName = $name -replace ' ', ''
$root = Split-Path -Parent $PSScriptRoot
$icon = Join-Path $root "brand/icons/$App.ico"

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$OutDir = (Resolve-Path $OutDir).Path
$staging = Join-Path ([System.IO.Path]::GetTempPath()) ("ferrum-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $staging | Out-Null

try {
    foreach ($binary in $meta.Binaries) {
        $exe = Join-Path $BinaryDir "$binary.exe"
        if (-not (Test-Path $exe)) {
            throw "$exe not found (build it first)"
        }
        Copy-Item $exe $staging
    }
    if (Test-Path $icon) {
        Copy-Item $icon (Join-Path $staging "$App.ico")
    } else {
        throw "$icon not found (run cargo brand)"
    }

    $zip = Join-Path $OutDir "$App-$Version-$Target.zip"
    if (Test-Path $zip) { Remove-Item $zip }
    Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $zip | Out-Null
    Write-Output $zip

    if ($SkipInstaller) { return }

    $iscc = $null
    $command = Get-Command iscc -ErrorAction SilentlyContinue
    if ($command) { $iscc = $command.Source }
    if (-not $iscc) {
        $iscc = Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"
    }
    if (-not (Test-Path $iscc)) {
        throw "Inno Setup (ISCC.exe) not found; install it or pass -SkipInstaller"
    }
    $outputName = "$fileName-$Version-x86_64-setup"
    & $iscc /Q `
        "/DAppId=$App" `
        "/DAppName=$name" `
        "/DAppSemver=$Version" `
        "/DAppNumericVer=$numericVersion" `
        "/DExeName=$($meta.Binaries[0]).exe" `
        "/DSourceDir=$staging" `
        "/DOutputDir=$OutDir" `
        "/DOutputName=$outputName" `
        (Join-Path $root "packaging/windows/ferrum.iss") | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "ISCC failed with exit code $LASTEXITCODE"
    }
    Write-Output (Join-Path $OutDir "$outputName.exe")
} finally {
    Remove-Item -Recurse -Force $staging -ErrorAction SilentlyContinue
}
