# Horizon — current layout

Source design: Fanatech, Horizon section `899:1923`, Figma file `r86Dn9BngVol84HXxRtt74`. The user's supplied Frame 1 (1), Frame 2 and Frame 3 images take precedence over earlier exports.

| Mode | Current dimensions |
| --- | --- |
| Compact lyric | 310 × 50 |
| Playing peek / paused dashboard | 591 × 101, centered in the host |
| Expanded pages | 680 × 280 by default; 32 px bottom corners |

The user requested a shorter expanded panel ending below the music transport controls, removal of the queue, a scrollable sidebar, more room for lyrics, and white icons. The expanded width increases the lyric column instead of stretching glyphs or cover art. Existing custom widths remain supported. Settings revision 2 changes the previous 591 px default to 680 once, retaining stored opacity and other preferences. Smaller modes retain their dimensions. Settings is deferred.

`src/figma_ui.rs` renders compact, both peek states, Music, Focus and Calendar natively. Water, Tasks and Shelf retain their feature rendering and adapt to the common 280 px height. The Win32 region and backing target use the same expanded height. Page icons are white on neutral glass plates; the first six sidebar pages scroll smoothly and Settings stays pinned. Task, water, calendar and focus data accents remain distinct from glyph colours.

Compact displays the synchronized active lyric centered over the timeline. Alerts take precedence; missing lyrics show loading/unavailable status. It uses the original smooth Samsung-style two-layer wave with cover-derived colour and 1.8× its original motion speed. Played width and playhead share one progress value, with no artificial minimum width. Pause freezes phase; disabling motion remains static. The right outlined bolt is white and opens the panel; it is not a battery indicator.

Frame 2 is the playing peek: cover (28,14), metadata x=108, divider x=276, lyric/timeline x=292, and playback button (518,29,40,39). Time labels are enlarged to 9.5 px at y=65 with left/right alignment. Frame 3 is the paused dashboard: plain translucent clock with no background glow/gradient, full date, task/water progress, a small song timeline, and the orange focus card with a Stop/Play pill. Its liquid is inset and clipped to the 11 px card corners, using the same #FF9317 fill as expanded Focus. Actual music playback controls the transport glyph: playing shows two hollow pause bars; paused shows play. White icons intentionally supersede the coloured icons in the references.

Expanded Music retains the 50 px transport controls, removes the queue area, and uses the extra right-hand width for measured lyric paragraphs. Expanded Music, compact, playing peek and the paused dashboard all use the same two-layer waveform renderer, cover colour, playback phase and rail/playhead geometry, sized for their slots. Focus moves its statistics and weekly progress above the new bottom edge. Calendar retains all 42 date cells on integer row positions: the 22 px marker and number share a centre, selected dates have an opaque purple fill with a reflected rim, and today retains a distinct outline. Two agenda items fit per page, and the native four-field event editor keeps save/cancel visible. Air, Tasks and Shelf retain their page content and use the same white sidebar icons as the other pages.

Calendar header controls place month/year at the left, Today next to them and month arrows at the right. Agenda pagination is grouped in a centered 116 × 24 control at y=228 with bottom padding. Timeline rails use 2.5 px in compact and 4 px in peek/expanded; the rail center and playhead remain aligned when thickness changes.

Water, Tasks and Shelf now use the available height: the neutral Water card and taller bottle end at y=264/250, task lists can show four rows with input controls pinned at y=230, and the taller file preview/card ends at y=264. Their existing data and actions are retained. Water backgrounds and controls are neutral; cyan remains a content/progress accent, and the Water page does not receive the album ambient wash.

Cover palettes sample a bounded 32 × 32 thumbnail and prefer meaningful chromatic accents over dominant dark/grey backgrounds. The original dark tone encoding is retained. Cover glows lift very dark colours enough to stay visible, span the actual widget bounds, and are also drawn behind the compact lyric. Compact and expanded music retain cover colour while paused; the paused dashboard clock remains free of a background effect.

Poppins Regular, SemiBold, Bold and ExtraLight are bundled; native edit controls and legacy pages retain Plus Jakarta Sans. SF Pro is not bundled. Original Figma exports remain in `assets/figma`; PNGs are build-time raster conversions. Static images are cached at their actual dimensions. Runtime code has no temporary Figma URLs, WebView or JavaScript dependency. Media, lyrics and cover imagery remain data driven.

Validation uses Clippy, unit tests, and native offscreen fixtures at 360/480/540/560/591/680 px. Renderer tests verify that coloured wave pixels never precede the playhead near track start. Native fixtures cover playing → paused dashboard → resumed music, stable transport hits, sidebar access and all agenda pages. `-CaptureLayouts` temporarily exports `layout-20261102-*`, `polish-591-page-*` and `polish-680-page-*`; delete only those owned debug captures after inspection. Keep runtime assets in `assets/figma`.
