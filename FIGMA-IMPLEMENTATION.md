# Horizon — current layout

Source design: Fanatech, Horizon section `899:1923`, Figma file `r86Dn9BngVol84HXxRtt74`. The user's supplied Frame 1 (1), Frame 2 and Frame 3 images take precedence over earlier exports.

| Mode | Current dimensions |
| --- | --- |
| Compact lyric | 310 × 50 |
| Playing peek / paused dashboard | 591 × 101, centered in the host |
| Expanded pages | 680 × 280 by default; 32 px bottom corners |

The user requested a shorter expanded panel ending below the music transport controls, removal of the queue, a scrollable sidebar, more room for lyrics, and white icons. The expanded width increases the lyric column instead of stretching glyphs or cover art. Existing custom widths remain supported. Settings revision 2 changes the previous 591 px default to 680 once, retaining stored opacity and other preferences. Smaller modes retain their dimensions. Settings is deferred.

`src/figma_ui.rs` renders compact, both peek states, Music, Focus and Calendar natively. Water, Tasks and Shelf retain their feature rendering and adapt to the common 280 px height. The Win32 region and backing target use the same expanded height. Page icons are white on neutral glass plates; the first six sidebar pages scroll smoothly and Settings stays pinned. Task, water, calendar and focus data accents remain distinct from glyph colours.

Compact displays the synchronized active lyric centered over the timeline. Alerts take precedence; missing lyrics show loading/unavailable status. It uses the original smooth Samsung-style two-layer wave with cover-derived colour and 1.8× its original motion speed. Played width and playhead share one progress value, with no artificial minimum width. Pause freezes phase; disabling motion remains static.

The compact left plate follows the most recently started/resumed focus or music
activity. A fresh reminder can show its icon, but a pending reminder never locks
out a later activation. Pausing that activity falls back to
the other running activity; metadata/automatic song changes do not claim
ownership. With no running activity, it uses the last selected page. Music uses
the actual cover when available; other contexts use neutral plates/white glyphs.
The right lightning is now a battery indicator: green fill follows Windows'
reported percentage, while glow/sparks indicate a connected adapter even when
the battery is not accepting current. Unknown or absent batteries stay neutral.
An AC power setting subscription refreshes the state promptly, backed by a
one-second status check. Known plug/unplug transitions show a 3.2-second notice
inside Horizon without adding reminders to the user's queue. Adapter animation
reuses the existing frame clock at 30 Hz when music does not need faster frames.

Active lyric text uses a glyph-based karaoke sweep. Word timings come from
Lyricsfile 1.0 when available; line-only records use interpolation between line
boundaries. Compact/medium overflow follows the highlighted glyph, so pause,
seek and track changes control the text position rather than a looping marquee.

The played rail uses the same colour as the front wave, and the unplayed rail
uses a dim shade of that cover tint, across compact, medium and expanded music.

Startup visibility is resolved before the first presentation. Hover from the
hidden indicator opens the medium view directly; its first content frame uses
the medium layout, with geometry interpolated from the actual indicator bounds.
A click opens the expanded panel. Pointer leave follows the existing policy:
compact lyrics while pinned, otherwise the indicator above another app. Fullscreen
hiding remains a separate policy. No intermediate compact layout is rendered
during an indicator-to-medium hover.

Frame 2 is the playing peek: cover (28,14), metadata x=108, divider x=276, lyric/timeline x=292, and playback button (518,29,40,39). Time labels are enlarged to 9.5 px at y=65 with left/right alignment. Frame 3 is the paused dashboard: plain translucent clock with no background glow/gradient, full date, task/water progress, a small song timeline, and the orange focus card with a Stop/Play pill. Its liquid is inset and clipped to the 11 px card corners, using the same #FF9317 fill as expanded Focus. Actual music playback controls the transport glyph: playing shows two hollow pause bars; paused shows play. White icons intentionally supersede the coloured icons in the references.

Expanded Music retains the 50 px transport controls, removes the queue area, and uses the extra right-hand width for measured lyric paragraphs. Expanded Music, compact, playing peek and the paused dashboard all use the same two-layer waveform renderer, cover colour, playback phase and rail/playhead geometry, sized for their slots. Focus moves its statistics and weekly progress above the new bottom edge. Calendar retains all 42 date cells on integer row positions: the 22 px marker and number share a centre, selected dates have an opaque purple fill with a reflected rim, and today retains a distinct outline. Two agenda items fit per page, and the native four-field event editor keeps save/cancel visible. Air, Tasks and Shelf retain their page content and use the same white sidebar icons as the other pages.

Calendar header controls place month/year at the left, Today next to them and month arrows at the right. Agenda pagination is grouped in a centered 116 × 24 control at y=228 with bottom padding. Timeline rails use 2.5 px in compact and 4 px in peek/expanded; the rail center and playhead remain aligned when thickness changes.

Water, Tasks and Shelf now use the available height: the neutral Water card and taller bottle end at y=264/250, task lists can show four rows with input controls pinned at y=230, and the taller file preview/card ends at y=264. Their existing data and actions are retained. Water backgrounds and controls are neutral; cyan remains a content/progress accent, and the Water page does not receive the album ambient wash.

Cover palettes sample a bounded 32 × 32 thumbnail and prefer meaningful chromatic accents over dominant dark/grey backgrounds. The original dark tone encoding is retained. Cover glows lift very dark colours enough to stay visible, span the actual widget bounds, and are also drawn behind the compact lyric. Compact and expanded music retain cover colour while paused; the paused dashboard clock remains free of a background effect.

Poppins Regular, SemiBold, Bold and ExtraLight are bundled; native edit controls and legacy pages retain Plus Jakarta Sans. SF Pro is not bundled. Original Figma exports remain in `assets/figma`; PNGs are build-time raster conversions. Static images are cached at their actual dimensions. Runtime code has no temporary Figma URLs, WebView or JavaScript dependency. Media, lyrics and cover imagery remain data driven.

Music transport plates and medium play/pause share a native opaque neutral
glass surface. A face reflection and one fading white rim sit underneath the
white glyph. Hover and press move the complete surface and glyph together;
they do not introduce another corner radius. The `*-glyph.svg`/PNG derivatives
remove the exported background rectangle; original exports remain unchanged.

Play uses its actual vector ink bounds for centering, rather than the SVG
viewport's asymmetric whitespace. The compact left activity plate also uses
the shared material: artwork is inset inside the 29 px plate, with reflection
drawn above it and one rim. Context glyphs stay white and share that same plate.

Expanded lyrics receive three soft cover-coloured radial glows behind the
text. An application seed selects smooth random paths for position, radius and
intensity; the same phase is stable on repeated paints. The existing frame
clock advances this phase only for playing music with lyrics, album colours
and motion enabled on the expanded Music page. Pause, seek and reopening do
not reset the glow. Three brushes are reused until the palette changes, and
overlapping brightness is bounded to preserve white lyric contrast. No new
timer, rendering thread, image or runtime dependency is introduced.

The medium dashboard clock uses a silver reflection gradient and a shallow
glyph bevel, without a background glow. Task rows, empty lists, input and
priority/add controls use neutral dark glass. The progress card alone keeps
the green surface (`#286B3B`); white/light type and the existing `#57FA65`
completion/check accents stay distinct from its fill. Single contour rims and
face reflections replace stacked outlines.

Native EDIT controls paint through a small buffered GDI surface whose alpha
is made fully opaque before composition. Typing, replacement and selection
repaint through the same path so they cannot punch holes in the parent shell.
The native editor retains its keyboard, clipboard, selection and IME behavior;
the task and calendar input colours match their opaque parent fields.

Shared interaction channels use analytic critically damped springs. Retargeting
preserves current value and velocity, and settled channels stop requesting
frames. Compact/medium geometry retains those springs through pointer reversals.
Page changes fade and drift the outgoing content in the sidebar direction while
leaving the sidebar and native hit rectangles stable. Sidebar selection blends
instead of snapping, and the plate/glyph share their hover/press offset. Glass
face reflections respond to the local pointer. Click pulses are mapped from
window to design coordinates for centered/scaled views. Motion-off remains
instant; the existing single UI frame clock and bounded caches are reused.

The shell contour also masks the Direct2D frame with per-pixel antialiasing.
DWM receives a premultiplied-alpha surface while the content stays opaque.
The matching Win32 region has a one-pixel guard so it cannot cut off fractional
edge coverage. One current geometry is cached across unchanged frames; no new
rendering thread or graphics device is added. If DWM frame extension fails,
the existing opaque target and integer region remain the fallback.

Validation uses Clippy, unit tests, and native offscreen fixtures at 360/480/540/560/591/680 px. Renderer tests verify that coloured wave pixels never precede the playhead near track start. Native fixtures cover playing → paused dashboard → resumed music, stable transport hits, sidebar access and all agenda pages. `-CaptureLayouts` temporarily exports `layout-20261102-*`, `polish-591-page-*` and `polish-680-page-*`; delete only those owned debug captures after inspection. Keep runtime assets in `assets/figma`.
