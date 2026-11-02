# Shared setup; callers choose which Cargo command to execute.
function Initialize-HorizonBuild {
    param([string]$ToolRoot = $env:HORIZON_TOOLCHAIN)
    $projectRoot = Split-Path -Parent $PSScriptRoot
    $localConfig = Join-Path $projectRoot '.local-build.json'
    if (-not $ToolRoot -and (Test-Path -LiteralPath $localConfig)) {
        $ToolRoot = (Get-Content -LiteralPath $localConfig -Raw | ConvertFrom-Json).toolRoot
    }
    if ($ToolRoot) {
        $env:CARGO_HOME = Join-Path $ToolRoot 'cargo'
        $env:RUSTUP_HOME = Join-Path $ToolRoot 'rustup'
        $cargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
        $rustc = Join-Path $env:CARGO_HOME 'bin/rustc.exe'
        if (-not (Test-Path -LiteralPath $cargo)) { throw "Cargo not found in tool root: $ToolRoot" }
        $env:PATH = (Split-Path -Parent $cargo) + [IO.Path]::PathSeparator + $env:PATH
    } else {
        $cargo = (Get-Command cargo -ErrorAction Stop).Source
        $rustc = (Get-Command rustc -ErrorAction Stop).Source
    }
    $sysroot = & $rustc --print sysroot
    if ($LASTEXITCODE) { throw 'Cannot locate the Rust sysroot.' }
    $versionInfo = & $rustc -vV
    if ($LASTEXITCODE) { throw 'Cannot query the Rust toolchain.' }
    if ($versionInfo -match '^host: x86_64-pc-windows-gnu$') {
        $nativeTools = Join-Path $sysroot 'lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained'
        $linker = Join-Path $nativeTools 'x86_64-w64-mingw32-gcc.exe'
        if (Test-Path -LiteralPath $linker) {
            $env:PATH = $nativeTools + [IO.Path]::PathSeparator + $env:PATH
            if (-not $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER) {
                $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $linker
            }
        }
    }
    return $cargo
}

function Get-HorizonVersion {
    $manifest = Get-Content -LiteralPath (Join-Path (Split-Path -Parent $PSScriptRoot) 'Cargo.toml') -Raw
    if ($manifest -notmatch '(?m)^version\s*=\s*"([^"]+)"') { throw 'Cargo package version is missing.' }
    return $Matches[1]
}
