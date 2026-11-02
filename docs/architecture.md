# Architecture

Horizon is a single native Windows executable without a WebView, JavaScript
runtime or .NET runtime.

| Area | Main modules |
| --- | --- |
| Win32 lifecycle, messages, host geometry and input | `main.rs`, `host.rs`, `input.rs`, `layout.rs`, `shell_policy.rs` |
| Direct2D/DirectWrite drawing, palettes and fonts | `render.rs`, `icons.rs`, `settings_icons.rs` |
| Page composition and interaction | `ui.rs`, `figma_ui.rs`, `tasks_ui.rs`, `shelf_ui.rs`, `focus_ui.rs` |
| Local state and time-based features | `model.rs`, `calendar.rs`, `reminders.rs`, `startup.rs` |
| Windows media sessions and artwork | `media.rs`, `artwork.rs` |
| Lyric requests and bounded persistence | `lyrics.rs`, `lyrics_disk.rs` |
| File shelf, transitions and native fixtures | `shelf.rs`, `motion.rs`, `probes.rs`, `polish_checks.rs` |

The Win32 message pump owns the window and renderer. Background work retrieves
media/lyrics and decodes previews; rendering and interaction updates stay on
their owning UI thread. `unsafe` Win32/COM calls require attention to handle
lifetime, thread affinity and buffer validity.

Settings and feature data are serialized locally. Saves flush a temporary file,
preserve a backup and use Windows replacement semantics. Lyrics have a separate
bounded cache. External service failure must not prevent the local UI from
working. See [privacy](../PRIVACY.md) for data and network behavior.

Technical debt includes large `main.rs`, `ui.rs` and `render.rs` modules,
coexisting legacy/Figma paths, and version-specific diagnostic output names.
Split these incrementally with fixture coverage. See [design notes](../FIGMA-IMPLEMENTATION.md)
for current geometry and rendering choices.
