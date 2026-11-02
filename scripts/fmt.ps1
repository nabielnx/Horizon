param([string]$ToolRoot = $env:HORIZON_TOOLCHAIN)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'toolchain.ps1')
Push-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
try {
    $cargo = Initialize-HorizonBuild -ToolRoot $ToolRoot
    & $cargo fmt
    if ($LASTEXITCODE) { throw 'Cargo fmt failed.' }
} finally { Pop-Location }
