[CmdletBinding()]
param(
    [string]$RpcUrl = $(if ($env:SOLANA_RPC_URL) { $env:SOLANA_RPC_URL } else { 'https://api.devnet.solana.com' }),
    [string]$Output
)

$ErrorActionPreference = 'Stop'
$scriptPath = Join-Path $PSScriptRoot 'verify_devnet_live.py'
$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
    throw 'Python 3 was not found. Install Python 3 and ensure python is on PATH.'
}

$arguments = @($scriptPath, '--rpc-url', $RpcUrl)
if ($Output) {
    $arguments += @('--output', $Output)
}

& $python.Source @arguments
if ($LASTEXITCODE -ne 0) {
    throw "PrazoPay live devnet verification failed with exit code $LASTEXITCODE."
}
