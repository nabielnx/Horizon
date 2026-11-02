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

`Horizon.exe --inspect-running-media` asks the already running instance to write
`artifacts/running-media-diagnostics.json`. It reads existing state without
controlling playback or starting lyric lookups. This is useful for intermittent
cover failures that do not reproduce in a fresh `--media-probe` process.

Karaoke uses Lyricsfile 1.0 word timestamps when provided by LRCLIB. Records with
only line timestamps use an interpolated sweep between line boundaries, which
cannot establish true word timing. Long idle lyrics scroll using the highlighted
glyph position from DirectWrite rather than an independent elapsed-time loop.
Legacy lyric caches remain readable and are refreshed in the background; failed
refreshes preserve existing lyrics. Offscreen pixel tests cover wrapping and
viewport clipping, without saving screenshots.

Compact battery status is read from Windows' `GetSystemPowerStatus`; unknown or
absent batteries have no invented percentage. AC adapter connection and actual
charging current are separate states: a connected adapter can have no charging
current, for example while holding a device charge limit. The lightning effect
uses adapter connection; diagnostics retain both fields.

An explicit `GUID_ACDC_POWER_SOURCE` registration receives `WM_POWERBROADCAST`
updates, with a one-second fallback refresh. Known plug/unplug edges show a
3.2-second in-app notice; initial unknown readings and capacity changes do not
create false notices. Adapter animation uses the shared UI
frame clock rather than a new polling/animation thread. Unit tests simulate
absent, unknown, charging, discharging and full battery reports; actual hardware
plug/unplug behavior still needs manual validation. Diagnostics include power
event registration, connection transition counts and current compact context.

Read-only media diagnostics (no player controls or lyric network requests):

```powershell
./artifacts/release-0.3.0/Horizon.exe --offscreen --media-probe
```

Add `--lyrics-network` explicitly to diagnose live lyric timing as well. This
sends the current track metadata to LRCLIB and may update the local lyric cache.
Reports include cover attempts/errors and lyric readiness under ignored
`artifacts`; review personal metadata before sharing them.

`verify.ps1 -SkipPreviews` runs offscreen startup, layout/settings and motion
probes using temporary data. The live lyric-network unit test is opt-in and
ignored by default. Offscreen tests do not prove compositor smoothness, actual
media-provider behavior, audio hardware support or startup after reboot.

`-CaptureLayouts` explicitly creates debugging images; it is not needed for CI.

The opt-in `desktop_compositor_preserves_shell_alpha` unit test creates two
small temporary windows on an unlocked desktop and samples their composed
pixels. Run it explicitly with `cargo test desktop_compositor -- --ignored`.
It checks fractional shell edges through the Win32 region, opaque content and
a dark native edit control before/after typing, selection replacement and a
calendar colour change; it saves no screenshots. A stock white EDIT does not
reproduce the alpha regression. Native GDI edits use
[buffered painting with opaque alpha](https://learn.microsoft.com/en-us/windows/win32/api/uxtheme/nf-uxtheme-bufferedpaintsetalpha)
on the extended frame. The visible probe is ignored in unattended CI,
along with the separate opt-in network test.

`check-palette.ps1` is an older pixel baseline that may need revision for newer
designs. Its legacy palette assertions are not current design acceptance.
Keep diagnostic output local and use synthetic data in reports.

## Releases

`Horizon.exe --offscreen --cover-fallback-probe` explicitly searches Apple's
catalogue for the current track and validates/downloads an exact matching image.
It writes `artifacts/cover-fallback-probe.json` and does not change playback.
This sends title/artist to Apple; it is never part of normal CI verification.
`--cover-fallback-on` asks the running instance to enable/save the opt-in setting.
Live media diagnostics retain twelve recent artwork errors for intermittent
failures, including failures before a successful retry.

`Horizon.exe --taskbar-layout-on` asks the running instance to enable and save
the top-taskbar overlay. It is idempotent; the same option can be changed in
Settings → Perilaku. Every size overlays the bar at the top edge. Verify actual
stacking with the installed taskbar/customization on the target desktop; an
offscreen test cannot prove Explorer's visible stacking behavior.

Build from an identified commit and rerun the checks. Review ZIP contents and
licenses, manually verify live Windows integrations, then tag the version matching
`Cargo.toml`. Attach the ZIP and SHA-256 file to a GitHub Release. CI uploads
build artifacts; it does not automatically publish a release. Packages are unsigned.
