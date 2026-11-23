# Horizon dusk companion

The full-body fantasy mascot has a crescent-shaped violet head, ivory face,
small relaxed body, flexible mitten hands and an amber crescent on its chest.
`approved-character-concept.png` is the approved design reference, not an app
capture. `mascot.svg` is its editable native vector adaptation.

The default pose lounges against the compact notch's lower-left arc. There are
three full-body postures: lounging, sitting upright, and curled up with the head
on the hands. Hip, head, arms, thighs and feet move together between postures;
the front sole remains grounded. Contacts stay stable within each pose.
Breathing affects the upper body; the head follows with a delay. The free arm uses two bones of constant
length, with an eased shoulder-to-wrist trajectory and delayed palm rotation.
`mascot-sleep.svg` is a separately authored side-sleeping key pose, with hands
under the head and bent knees/feet close to the body. Its contour commands match
the lounge master so every control point can interpolate continuously. Head and
free-hand joints follow those contacts; no seated silhouette is rotated to fake
sleep. Only the current body/material curves are retained; no animation
atlas, video, external model or runtime image service is used.

Quiet compact mode contains the mascot, glass clock and passive battery status,
in a 310x42 notch or 270x42 floating island. Active music/focus retain their
contextual icons and the right indicator follows that activity. Charger notices
preserve the left identity; playback controls appear in medium/expanded mode. Reveals greet,
pointer entry wakes, and clicks make the character smile. The idle cycle adds
curiosity, smiles and yawns before sleeping after 33 seconds without interaction.
Sleep persists until hover/click or a reveal following another activity. Only
slow breathing and a small resting head movement continue during sleep. Rest weights
blend rather than switching abruptly. Hidden and disabled-motion states stop
animation, and offscreen layout fixtures use a stable pose.
Eye wake-up and body unfolding have separate timing; the free hand waits for
the body to unfold before greeting, so wake-up does not snap between drawings.

Palette: violet `#695276` / `#35293F`, ivory `#F6E9E2` / `#CDB8C7`,
amber `#FFD38B` / `#D79551`. Fine edge highlights suggest the app's glass material.

Horizon uses two mirrored glass eyes as its app icon. `eye.svg` defines the
shape shared by the animated eyes and the icon generator. `horizon.ico`
contains 16, 20, 24, 32, 40, 48, 64, 128 and 256px images. Settings uses the
matching `app-icon.png` in its header. Regenerate both after editing the icon:

```powershell
npm ci --prefix scripts
npm run --prefix scripts prepare-branding
```

Cargo embeds the committed ICO as standard PE resources through `build.rs`.
Node and a resource compiler are not required for ordinary Cargo builds.
