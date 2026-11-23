# Changelog

## 0.3.0 — Unreleased

- Reduce compact height to 42px, keeping its width and fitting its floating
  5px gap inside a 48px top taskbar. Recenter artwork/status, reflow lyrics and
  resize the quiet mascot/clock; retain medium and expanded panel dimensions.

- Keep mouse volume dragging separate from keyboard focus so releasing the
  slider leaves no persistent outline. Clear keyboard focus when the window
  loses focus and repaint when pointer capture ends.

- Show passive music, focus, reminder or idle battery indicators in compact
  mode; keep playback controls in the panel opened on hover. Charger notices
  preserve the left activity/mascot and use a connection pulse without crosses.

- Paint cover glow through its complete radial falloff instead of cropping a
  colored rectangle inside the player; keep clipping at the outer shell only.

- Use a neutral grey-white waveform and track while the cover palette is
  unavailable, then use the extracted cover color when ready.

- Prefer a current-user Task Scheduler logon trigger with no configured delay,
  interactive/non-elevated execution and battery-friendly settings. Remove the
  legacy Horizon Run entry after successful registration; retain Run fallback
  if Task Scheduler is unavailable. Disabling startup removes both mechanisms.
- Prepare the first panel before starting media work or checking registration.
  Add a 420ms fade/slide entrance that respects surface opacity and reduced
  motion, retaining the completed first frame before showing the window.

- Add saved Indonesian/English UI language selection with immediate label,
  calendar, status, reminder and native input-cue updates. User-authored text,
  file names and media/lyric content retain their original language.
- Add independent opacity controls for six feature pages, compact/medium idle
  and Settings, plus an explicit all-surfaces reset. Apply opacity consistently
  to native windows, including Music, Calendar, Tasks and idle.
- Improve Settings contrast using deeper backgrounds, clearer cards and a
  stronger neutral selection state, while retaining white icons.

- Add a saved Notch / Dynamic Island choice under Appearance. Island mode uses
  a shorter 270px compact panel with nearly semicircular ends, rounded
  medium/expanded contours and a 5px top gap. Switching
  styles animates both the shared render/hit-test contour and window position;
  reduced motion applies the selected style immediately.
- Give compact lyrics more room by spacing the cover and battery outward.
  Tune medium corner radii independently for attached and floating styles,
  with continuous corner interpolation during shell transitions.

- Keep compact idle visible over ordinary apps in taskbar-overlay mode, even
  without playing music. Repair stacking above the active app as well as the
  taskbar without activating Horizon. Recognize maximized frameless desktop
  window controls while retaining true fullscreen hiding.

- Distinguish slow/unreachable lyric services from completed searches without a
  synchronized match. Continue to search after an exact-request transport error,
  bound each lookup with an eight-second network budget, and retain per-route
  timing/status diagnostics. Respect server cooldowns and reject stale tracks.
- Retain successful lyrics for 30 days in a bounded 256-song/8 MiB disk cache;
  keep the hot memory cache at 32 songs/2 MiB and reload older songs on demand.

- Add a full-body dusk mascot with a crescent head, violet/ivory materials and
  an amber chest accent. Its seated pose follows the compact notch's lower arc;
  quiet compact mode contains only the character and a larger glass clock.
- Animate breathing, an articulated free arm with a delayed wrist, head motion
  and facial expressions. Lounge, upright seated and curled sleeping postures
  move the hip, head, support arm, thighs and feet with smooth body transitions.
  Wake-up unfolds the body before raising the free hand to greet.
- Use a separately authored side-sleeping body key pose, with folded knees and
  hands supporting the head. Matching contour topology keeps entry/exit smooth.
  Reveals greet the user; unattended sleep persists until interaction. Hidden and disabled
  motion states stop animation. Active music/focus icons retain priority.
- Embed the new crescent-head EXE/window icon in nine sizes, from 16 to 256px.

- Read Windows thumbnails explicitly from byte zero, preserve validated artwork
  through album-only metadata changes and brief missing-session gaps, and keep
  bounded recent failure diagnostics across successful retries.
- Add opt-in Apple/iTunes cover fallback when Windows artwork remains missing,
  with strict recording/release matching, stale-track rejection, bounded memory
  caching and clickable Apple Music attribution. Windows artwork has priority.

- Add an opt-in top-taskbar overlay for compact, medium and expanded Horizon.
  Foreground notifications and the existing policy refresh repair its stacking
  order without taking focus, moving Explorer or adding an animation timer.

- Rebuild all seven Settings pages with consistent white icons, neutral glass
  cards, aligned value controls, animated switches and an automatic-save footer.
  Settings interactions now share the UI animation clock. Radius changes affect
  the actual shell and preview, and opacity labels identify the affected pages.

- Use velocity-preserving spring motion for shared hover, press, selection and
  progress channels, including reversible compact/medium shell transitions.
- Add directional page transitions, animated sidebar selection/press and
  pointer-reactive glass reflections. Correct click feedback coordinates in
  centered/scaled views and present the initial mouse press immediately.

- Repair transparent dark native inputs on the alpha shell with buffered opaque
  EDIT painting, including typing, replacement, selection and calendar fields.
- Use reflective silver glyphs for the medium dashboard clock. Task lists,
  input and controls use neutral dark glass; the progress card keeps a brighter
  green fill and stronger face/rim reflections with readable light typography.

- Center the actual play glyph in medium and expanded music controls, including
  hover/press offsets and narrower layouts.
- Add three softly moving, cover-coloured glows behind expanded lyrics. Smooth
  random paths reuse the existing frame clock and cached brushes; pause and
  disabled motion freeze the effect without resetting its position.
- Compact activity icons and artwork share a glass plate with an inset cover,
  face reflection and one fading rim instead of stacked solid outlines.

- Music transport buttons share one opaque glass surface with a fading white
  reflection, including play/pause in medium mode. Glyph-only assets prevent
  baked-in fills and mismatched hover outlines from stacking.
- Shell corners use an antialiased alpha mask through DWM, with a guarded
  Win32 region, preserving the existing dimensions and native input controls.

- Played music rails use the front waveform colour in compact, medium and
  expanded views; the remaining rail uses a dim shade of the same cover tint.

- Compact focus/music ownership follows the latest start/resume action; pausing
  an activity falls back to another running activity instead of locking focus.
  Brief metadata gaps and already-pending reminders do not steal that ownership.
- Adapter connection is tracked independently from charging current, including
  devices that hold a charge limit. Plug/unplug shows a short in-app notice and
  updates lightning effects through an explicit AC power event subscription.

- Compact artwork/icon follows the active context. Idle battery status shows
  Windows charge as green fill, with a brief glow when the adapter connects.

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
