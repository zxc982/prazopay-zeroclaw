[CmdletBinding()]
param(
    [string]$Distro = 'Ubuntu-24.04'
)

$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$wslHome = (& wsl.exe -d $Distro -- bash -lc 'printf %s "$HOME"').Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($wslHome)) {
    throw "Could not resolve the Linux home directory for $Distro."
}
$toolPath = "$wslHome/.local/share/solana/install/active_release/bin:$wslHome/.cargo/bin:$wslHome/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

& wsl.exe -d $Distro --cd $projectRoot -- /usr/bin/env "PATH=$toolPath" bash ./scripts/day1-check.sh
if ($LASTEXITCODE -ne 0) {
    throw "PrazoPay Day 1 check failed with exit code $LASTEXITCODE."
}
