# Changelog

## 0.3.0 — Unreleased

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
