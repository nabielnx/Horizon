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

Successful lyrics can be cached in `%APPDATA%\Horizon\lyrics-cache.json` for
up to 24 hours, bounded to 32 records and 2 MiB. Close Horizon and delete that
file to remove the persistent cache. Removing the Horizon data directory resets
stored application data. Disable "start with Windows" before uninstalling so the
per-user startup registration is also removed.

There is no Horizon account, hosted database, analytics SDK or crash-upload
service in the current source. Diagnostic flags write local reports into
`artifacts`; media diagnostics can include track information and paths. Review
reports before sharing them. Repository fixtures contain synthetic data.
