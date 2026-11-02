param([string]$ToolRoot = $env:HORIZON_TOOLCHAIN)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'scripts/toolchain.ps1')
Push-Location -LiteralPath $PSScriptRoot
try {
    $cargo = Initialize-HorizonBuild -ToolRoot $ToolRoot
    & $cargo fmt --check
    if ($LASTEXITCODE) { throw 'Formatting check failed.' }
    & $cargo clippy --locked --all-targets -- -D warnings
    if ($LASTEXITCODE) { throw 'Clippy failed.' }
    & $cargo test --locked
    if ($LASTEXITCODE) { throw 'Tests failed.' }
    & $cargo build --locked --release
    if ($LASTEXITCODE) { throw 'Build failed.' }

    $version = Get-HorizonVersion
    $packageDir = Join-Path $PSScriptRoot "artifacts/release-$version"
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    $metadata = & $cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE) { throw 'Cargo metadata failed.' }
    $targetRoot = $metadata.target_directory
    if ($env:CARGO_BUILD_TARGET) { $targetRoot = Join-Path $targetRoot $env:CARGO_BUILD_TARGET }
    Copy-Item -LiteralPath (Join-Path $targetRoot 'release/horizon.exe') -Destination (Join-Path $packageDir 'Horizon.exe')
    foreach ($file in @('README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'PRIVACY.md')) {
        Copy-Item -LiteralPath $file -Destination $packageDir
    }
    Copy-Item -LiteralPath 'assets/fonts' -Destination $packageDir -Recurse -Force
    $licenseDir = Join-Path $packageDir 'licenses'
    New-Item -ItemType Directory -Path $licenseDir -Force | Out-Null
    Copy-Item -LiteralPath 'assets/icons/LICENSE.txt' -Destination (Join-Path $licenseDir 'phosphor.txt')
    $archive = Join-Path $PSScriptRoot "artifacts/Horizon-$version-win-x64.zip"
    Compress-Archive -Path (Join-Path $packageDir '*') -DestinationPath $archive -Force
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($archive))" | Set-Content -LiteralPath "$archive.sha256" -Encoding ascii
    Write-Output "Ready: $packageDir/Horizon.exe"
    Write-Output "Archive: $archive"
} finally { Pop-Location }
