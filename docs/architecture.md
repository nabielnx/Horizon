# Architecture

Horizon is a single native Windows executable without a WebView, JavaScript
runtime or .NET runtime.

| Area | Main modules |
| --- | --- |
| Win32 lifecycle, messages, host geometry and input | `main.rs`, `host.rs`, `input.rs`, `layout.rs`, `shell_policy.rs` |
| Direct2D/DirectWrite drawing, palettes and fonts | `render.rs`, `icons.rs`, `settings_icons.rs` |
| Dusk companion, grounded full-body rig and material curves | `mascot.rs`, `mascot_rig.rs`, `mascot_render.rs`, `mascot_deform.rs`, `assets/branding` |
| Page composition and interaction | `ui.rs`, `figma_ui.rs`, `tasks_ui.rs`, `shelf_ui.rs`, `focus_ui.rs` |
| Local state and time-based features | `model.rs`, `calendar.rs`, `reminders.rs`, `startup.rs` |
| Windows media sessions and artwork | `media.rs`, `artwork.rs` |
| Lyric requests and bounded persistence | `lyrics.rs`, `lyrics_disk.rs` |
| File shelf, transitions and native fixtures | `shelf.rs`, `motion.rs`, `probes.rs`, `polish_checks.rs` |

The Win32 message pump owns the window and renderer. Background work retrieves
media/lyrics and decodes previews; rendering and interaction updates stay on
their owning UI thread. `unsafe` Win32/COM calls require attention to handle
lifetime, thread affinity and buffer validity.

The companion shares the existing animation clock and visibility policy. Its
geometry and brushes are retained per render target; a two-bone arm solve,
anchored breathing torso and delayed head/wrist gestures produce its poses.
The editable brand SVG also produces the committed multi-size
ICO. Cargo embeds that icon as a PE resource without additional runtime files
or build dependencies. See [branding](../assets/branding/README.md).

Settings and feature data are serialized locally. Saves flush a temporary file,
preserve a backup and use Windows replacement semantics. Lyrics have a separate
bounded cache. External service failure must not prevent the local UI from
working. See [privacy](../PRIVACY.md) for data and network behavior.

`i18n.rs` and the embedded English catalog localize application-owned copy.
Formatted copy uses checked Rust format branches, preserving user values;
native edit cues refresh without overwriting input. Language changes require
no network or restart. Nine optional opacity overrides fall back to the saved
base opacity; editing the all-surfaces value explicitly clears the overrides.
The embedded common-controls v6 manifest enables cue-banner support for EDIT.

Lyrics use an eight-second per-lookup network budget with phase timeouts
recomputed from the remaining time. Transport errors allow the next search
route; provider errors and Retry-After stop the lookup. An incomplete lookup
never enters the negative cache. Current-track diagnostics retain at most five
route timings/statuses, without lyric text. Successful lyrics remain on disk
for up to 30 days (256 records/8 MiB), independently of the 32-record/2 MiB hot
memory cache. Cache history is merged on write and hydrated on demand.

Technical debt includes large `main.rs`, `ui.rs` and `render.rs` modules,
coexisting legacy/Figma paths, and version-specific diagnostic output names.
Split these incrementally with fixture coverage. See [design notes](../FIGMA-IMPLEMENTATION.md)
for current geometry and rendering choices.
