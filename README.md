# Horizon

A native Windows notch for music, synchronized lyrics and everyday utilities.
Built with Rust, Win32, Direct2D, DirectWrite and WIC, without a WebView or .NET
runtime.

Horizon is pre-1.0 software. The current source version is **0.3.0**.

## Features

- Compact lyric view, media peek and an expanded panel with a scrollable sidebar.
- Windows media controls, cover artwork, cover-derived palettes and LRCLIB lyrics.
- Notch or floating Dynamic Island style, selectable in Settings → Tampilan.
- Indonesian/English interface and independent page, idle and Settings opacity.
- Focus timer, calendar events/reminders and water tracking.
- Tasks with priorities/progress and a local file shelf with previews.
- Assistant with your own provider API key, file attachments, and task suggestions.
- Local settings, configurable behavior and optional start with Windows.
- Startup uses a per-user logon task without an added delay when available,
  with a standard Windows Run fallback. No administrator privileges or stored
  password are required; Windows startup load still affects launch timing.
- Shared waveform/motion rendering and native offscreen verification.
- Karaoke highlighting with word timing when available, timeline-driven overflow
  lyrics, contextual compact icons, passive activity indicators and idle battery status.
- Animated glass eyes beside the idle clock. The executable and Settings
  use the same pair of eyes as the app icon. Disable motion for a static pose.

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

To use the Assistant, scroll the expanded sidebar to **Asisten**. Add your
API key in **Settings → AI** and choose OpenAI, Gemini, OpenRouter, Groq,
Claude, or a custom HTTPS Chat Completions endpoint. Enter the provider's model
ID and save. OpenRouter starts with `openrouter/free`. Each provider has its
own quota and terms. Attach UTF-8 TXT or Markdown, or use PDF with OpenAI,
Gemini, OpenRouter or Claude and a PDF-capable model. The attachment limit is
4 MB per file and 12 MB total across up to eight attachments. Use **Attach**
to select multiple files, paste images with Ctrl+V, or select **Tanya AI** on
a supported file in the shelf. Image attachments need a vision-capable model.
The microphone button uses Windows dictation to fill the message box.

### Spotify search and playback

With Spotify Free, select the magnifying glass in the expanded Music panel,
enter a title or artist, and select **Search in Spotify desktop**. Choose
the song in the Spotify app. **Open in browser** opens Spotify Web Player
in the default browser. Horizon can show playback when the player exposes
its media session to Windows.

To play tracks directly from Horizon with Spotify Premium:

1. Create a Web API app in the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard)
   and add `http://127.0.0.1:43871/spotify/callback` as a redirect URI.
2. Open the expanded Music page in Horizon, select the magnifying glass, and
   select **Set up Premium**.
   Paste the app's **Client ID**, then press Enter or select the sign-in button.
   Finish signing in with Spotify in your browser.
3. Keep Spotify desktop open on the same Premium account. Search by song or
   artist in Horizon and press the play button beside a result.

In Spotify development mode, each account needs access to the app in the Dashboard.
If you have several Spotify computers connected, make this PC the active Spotify
device before playing a result. Select **Disconnect Spotify** in the search panel
to remove the saved session from this computer.

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
