# Development

## Requirements

- Windows 10/11 x64; Horizon uses Win32, Direct2D, DirectWrite and WIC.
- Stable Rust installed with rustup, including `rustfmt` and `clippy`.
- For the standard MSVC toolchain: Visual Studio Build Tools with Desktop
  development with C++ and the Windows SDK. GitHub Windows runners supply these.
- PowerShell 5.1 or PowerShell 7.

The existing developer build was verified with Rust 1.99.1 on
`x86_64-pc-windows-gnu`. CI exercises the Windows runner's standard MSVC host.
`rust-toolchain.toml` follows stable; this is not a promise of bit-identical
binaries across compiler versions. `Cargo.lock` fixes Rust dependency resolution.

## Build and run

```powershell
git clone https://github.com/nabielnx/Horizon.git
cd Horizon
./build.ps1
./artifacts/release-0.3.0/Horizon.exe
```

The build checks formatting, Clippy with warnings denied, unit tests, and release
compilation using the lockfile. It creates a versioned folder, ZIP and SHA-256
file in `artifacts`. Keep `fonts` beside the executable. Adjust the example path
when changing the package version.

Individual commands:

```powershell
./scripts/check.ps1
./scripts/fmt.ps1
./scripts/test.ps1
./scripts/verify.ps1 -SkipPreviews
```

Scripts also work from another current directory. An optional
`HORIZON_TOOLCHAIN`, `-ToolRoot` argument or ignored `.local-build.json` can
select a portable Rust installation with `cargo` and `rustup` subdirectories.
Normal builds require no private machine path. GNU installations need a working
linker; the helper detects bundled self-contained GNU tools where present.
`.cargo/config.toml` remains machine-local.

## Optional asset conversion

Committed SVG/PNG assets are sufficient to build. Node.js is only needed to
regenerate PNGs after changing Figma SVG exports:

```powershell
npm ci --prefix scripts --ignore-scripts
npm run --prefix scripts prepare-figma
```

The converter uses pinned `@resvg/resvg-js` and a committed npm lockfile rather
than the ignored `artifacts` directory as a source dependency.

## Verification limits

`verify.ps1 -SkipPreviews` runs offscreen startup, layout/settings and motion
probes using temporary data. The live lyric-network unit test is opt-in and
ignored by default. Offscreen tests do not prove compositor smoothness, actual
media-provider behavior, audio hardware support or startup after reboot.

`-CaptureLayouts` explicitly creates debugging images; it is not needed for CI.
`check-palette.ps1` is an older pixel baseline that may need revision for newer
designs. Its legacy palette assertions are not current design acceptance.
Keep diagnostic output local and use synthetic data in reports.

## Releases

Build from an identified commit and rerun the checks. Review ZIP contents and
licenses, manually verify live Windows integrations, then tag the version matching
`Cargo.toml`. Attach the ZIP and SHA-256 file to a GitHub Release. CI uploads
build artifacts; it does not automatically publish a release. Packages are unsigned.
