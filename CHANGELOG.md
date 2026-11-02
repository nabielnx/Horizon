# Changelog

## 0.3.0 — Unreleased

- Played music rails use the front waveform colour in compact, medium and
  expanded views; the remaining rail uses a dim shade of the same cover tint.

- Compact focus/music ownership follows the latest start/resume action; pausing
  an activity falls back to another running activity instead of locking focus.
  Brief metadata gaps and already-pending reminders do not steal that ownership.
- Adapter connection is tracked independently from charging current, including
  devices that hold a charge limit. Plug/unplug shows a short in-app notice and
  updates lightning effects through an explicit AC power event subscription.

- Compact artwork/icon follows the active context. The right lightning shows
  Windows battery charge as green fill and adapter connection as glow/sparks.

- Karaoke highlighting uses Lyricsfile word timestamps when available, with a
  line-timed sweep fallback. Long compact/medium lyrics follow the sung glyph
  inside their viewport; pause and seek use the media timeline directly.
- Recheck artwork after media-property events, shortly after a track change,
  and periodically to recover late thumbnails without discarding valid artwork.
- Add opt-in diagnostics for the actual running instance, including thumbnail
  acquisition errors and cover-render failures.

- First hover opens the medium view directly from the hidden indicator, with
  geometry starting at the actual visible bounds; a click opens the large panel.
- Cover retrieval uses a separate bounded worker, partial stream reads and
  deadline cancellation; temporary metadata failures preserve valid artwork.
- Lyric lookups reuse their HTTP connection, prefer exact records even before
  duration is available, apply positive cache entries immediately and respect
  server `Retry-After` responses. Missing results are retried after 30 seconds.

- Native Windows notch with compact, media peek and expanded views.
- Windows media controls, cover-based palettes and synchronized LRCLIB lyrics.
- Focus timer, calendar reminders, water tracking, tasks and local file shelf.
- Local settings and persistence with backup/replacement behavior.
- Shared waveform/motion rendering with offscreen verification probes.
- Initial repository setup: build documentation, Windows CI, dependency locks,
  license notices and source/release separation.

This is the initial imported source. It does not assert that a tagged release
has been published or that all live integrations have been verified.
