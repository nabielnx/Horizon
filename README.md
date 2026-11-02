# Horizon

A native Windows notch for music, synchronized lyrics and everyday utilities.
Built with Rust, Win32, Direct2D, DirectWrite and WIC, without a WebView or .NET
runtime.

Horizon is pre-1.0 software. The current source version is **0.3.0**.

## Features

- Compact lyric view, media peek and an expanded panel with a scrollable sidebar.
- Windows media controls, cover artwork, cover-derived palettes and LRCLIB lyrics.
- Focus timer, calendar events/reminders and water tracking.
- Tasks with priorities/progress and a local file shelf with previews.
- Local settings, configurable behavior and optional start with Windows.
- Shared waveform/motion rendering and native offscreen verification.
- Karaoke highlighting with word timing when available, timeline-driven overflow
  lyrics, contextual compact icons and a Windows battery/charging indicator.

## Run

Requires **Windows 10/11 x64**. Extract a package from this repository's Releases
when available, then run `Horizon.exe`. Keep the bundled `fonts` directory beside
the executable. Packages are currently unsigned.

Click the notch to expand/collapse it. Use the Settings icon or right-click to
open settings; `Ctrl+Alt+H` also toggles the notch. Exit through **Settings →
About → Exit** before replacing a running executable.

Media integration depends on the player's Windows media-session support. Lyrics
require an enabled lookup, network access, and a matching LRCLIB entry. The wave
is decorative animation, not an audio amplitude measurement.

## Build

Install stable Rust and Windows C++ build tools, then use PowerShell:

```powershell
git clone https://github.com/nabielnx/Horizon.git
cd Horizon
./build.ps1
./scripts/verify.ps1 -SkipPreviews
```

The build checks formatting, Clippy and tests, then creates
`artifacts/release-0.3.0/Horizon.exe`, a ZIP and SHA-256 checksum. The source
includes its required assets; Node.js is only needed for optional asset
regeneration. See [development](docs/development.md) for prerequisites and limits.

## Data and privacy

Application data is stored in `%APPDATA%\Horizon`. Enabled lyric lookup sends
track title, artist and duration to LRCLIB over HTTPS. Tasks, calendar entries
and shelf paths stay local in the current implementation. Read [PRIVACY.md](PRIVACY.md)
before sharing diagnostic reports or backing up data.

## Project documentation

- [Architecture](docs/architecture.md) and [current layout](FIGMA-IMPLEMENTATION.md)
- [Development and verification](docs/development.md)
- [Repository audit and remaining work](docs/repository-audit.md)
- [Contributing](CONTRIBUTING.md), [security](SECURITY.md) and [changelog](CHANGELOG.md)

Offscreen checks cover synthetic state, layout and motion. Live media sessions,
lyric-service behavior, startup after reboot and visible compositor smoothness
still need manual validation. Memory usage varies with media and previews;
there is no fixed RAM guarantee.

## License

Application source is [MIT licensed](LICENSE). Fonts, icons and other third-party
material retain their own terms; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
