[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [string]$Binary,
    [string]$OutputDir,
    [ValidateSet("x86_64", "aarch64")]
    [string]$Architecture = "x86_64"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Desktop = Join-Path $Root "apps/desktop"
$Manifest = Join-Path $Desktop "windows/Cargo.toml"
$InstallerScript = Join-Path $Desktop "windows/packaging/aimemorandum.nsi"

if (-not $OutputDir) {
    $OutputDir = Join-Path $Desktop "dist"
}
$OutputDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDir)
$null = New-Item -ItemType Directory -Force -Path $OutputDir

if (-not $Binary) {
    $Binary = Join-Path $Desktop "target/release/aimemorandum-windows.exe"
    if (-not $SkipBuild) {
        & cargo build --release --manifest-path (Join-Path $Desktop "Cargo.toml") -p aimemorandum-windows
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }
}
$Binary = (Resolve-Path -LiteralPath $Binary).Path
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "Windows executable not found: $Binary"
}

$VersionMatch = Select-String -LiteralPath $Manifest -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $VersionMatch) {
    throw "Cannot read package version from $Manifest"
}
$Version = $VersionMatch.Matches[0].Groups[1].Value
$NumericVersion = ($Version -split '-', 2)[0]
$VersionParts = $NumericVersion.Split('.')
if ($VersionParts.Count -ne 3 -or ($VersionParts | Where-Object { $_ -notmatch '^\d+$' })) {
    throw "NSIS requires a numeric three-part package version, got: $Version"
}
$FileVersion = "$NumericVersion.0"

foreach ($RequiredFile in @((Join-Path $Root "LICENSE"), (Join-Path $Root "THIRD_PARTY_NOTICES.md"), $InstallerScript)) {
    if (-not (Test-Path -LiteralPath $RequiredFile -PathType Leaf)) {
        throw "Required packaging input not found: $RequiredFile"
    }
}

$Compiler = $env:MAKENSIS
if ([string]::IsNullOrWhiteSpace($Compiler)) {
    $Command = Get-Command makensis -ErrorAction SilentlyContinue
    if (-not $Command) {
        $Command = Get-Command makensis.exe -ErrorAction SilentlyContinue
    }
    if (-not $Command) {
        throw "NSIS compiler not found. Set MAKENSIS or put makensis on PATH."
    }
    $Compiler = $Command.Source
}

$Output = Join-Path $OutputDir "AIMemorandum-$Version-$Architecture-setup.exe"
$Arguments = @(
    "/V3",
    "/DAPP_VERSION=$Version",
    "/DAPP_ARCH=$Architecture",
    "/DFILE_VERSION=$FileVersion",
    "/DBINARY_PATH=$Binary",
    "/DLICENSE_PATH=$(Join-Path $Root 'LICENSE')",
    "/DNOTICES_PATH=$(Join-Path $Root 'THIRD_PARTY_NOTICES.md')",
    "/DOUTPUT_PATH=$Output",
    $InstallerScript
)
& $Compiler @Arguments
if ($null -ne $LASTEXITCODE -and $LASTEXITCODE -ne 0) {
    throw "makensis failed with exit code $LASTEXITCODE"
}
if (-not (Test-Path -LiteralPath $Output -PathType Leaf)) {
    throw "makensis did not create $Output"
}
Write-Host "Built $Output"
