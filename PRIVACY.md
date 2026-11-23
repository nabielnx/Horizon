# Privacy and local data

Horizon stores settings, tasks, calendar events, focus/water state, favorites and
file-shelf paths in `%APPDATA%\Horizon\data.json`. A `.bak` copy may be kept
for recovery. These files are not encrypted; access follows the Windows user
profile's permissions. Close Horizon before backing up or removing them.

Windows media-session APIs provide track metadata, cover, playback position and
transport controls. Available information depends on the player. The file shelf
reads local paths and creates previews locally.

When lyric lookup is enabled, Horizon sends track title, artist and duration to
[LRCLIB](https://lrclib.net) over HTTPS. The service also receives normal connection
information, including the source IP address. Its availability and data practices
are outside Horizon's control. Disable lookup in Music settings to stop new
requests; requests already in progress may finish.

The optional **Cover cadangan (Apple)** setting sends title, artist and the
Windows region code to Apple's iTunes Search API over HTTPS only when Windows
artwork remains missing. Horizon tries that region first and can also check the
US catalogue.
Matching checks album/version and duration locally. Matching images are fetched
from Apple's `mzstatic.com` CDN; Apple also receives normal connection data.
The fallback defaults off and can be disabled in Music settings. Its artwork
cache is memory-only, limited to four images and 1 MiB. Windows artwork takes
priority. A clickable Apple Music source link is shown when a fallback is used.

Spotify search is optional. Horizon passes the submitted query to Spotify
desktop or opens it in Spotify Web Player when you select **Open in browser**.
If you connect Spotify Premium, Horizon opens Spotify sign-in
in your browser and receives the authorization code on a temporary loopback
listener. It sends submitted search terms and the selected track and playback
device IDs to Spotify over HTTPS. The public Client ID is stored in local
settings and the refresh token is stored in Windows Credential Manager.
Disconnect Spotify in the search panel to remove the saved session from Windows.

The optional Assistant sends your prompt, recent chat turns for the same
connection, and any attached file to the selected AI provider over HTTPS when
you select Send. Horizon reads the attached file for that request. Keys are
stored separately in Windows Credential Manager for each provider or custom
endpoint and can be removed in Settings → AI. Chat text, model identifiers,
and attachment names are saved in `%APPDATA%\Horizon\assistant-chats.json`.
Delete conversations in History. Pasted image bytes stay in memory until
sent or removed. Asisten dictation uses Windows speech recognition and may
send audio to Microsoft's online speech service. Horizon sends the resulting
text to your chosen AI provider when you select Send.
Horizon sets `store: false` for OpenAI Responses API requests. The selected
provider's retention and routing policies apply.

Successful lyrics can be cached in `%APPDATA%\Horizon\lyrics-cache.json` for
up to 30 days, bounded to 256 records and 8 MiB on disk. The hot memory cache
remains limited to 32 records and 2 MiB. Close Horizon and delete that
file to remove the persistent cache. Removing the Horizon data directory resets
stored application data. Disable "start with Windows" before uninstalling so the
per-user startup registration is also removed.

Startup registration uses a Horizon-only Task Scheduler logon task for your
account, running interactively without elevated privileges or a stored password.
If Task Scheduler is unavailable, Horizon uses its per-user Run entry instead.
Disabling start with Windows removes Horizon's task and Run entry.

There is no Horizon account, hosted database, analytics SDK or crash-upload
service in the current source. Diagnostic flags write local reports into
`artifacts`; media diagnostics can include track information and paths. Review
reports before sharing them. Repository fixtures contain synthetic data.
