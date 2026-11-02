param([string]$ToolRoot = $env:HORIZON_TOOLCHAIN)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'toolchain.ps1')
Push-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
try {
    $cargo = Initialize-HorizonBuild -ToolRoot $ToolRoot
    & $cargo check --locked
    if ($LASTEXITCODE) { throw 'Cargo check failed.' }
} finally { Pop-Location }
