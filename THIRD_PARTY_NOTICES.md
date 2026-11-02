# Third-party notices

Horizon source code is MIT licensed. Third-party material retains its own terms.

| Material | Upstream | License and local notice |
| --- | --- | --- |
| SVG icons in `assets/icons` | [Phosphor Icons](https://github.com/phosphor-icons/core) | MIT; `assets/icons/LICENSE.txt` |
| Plus Jakarta Sans | [Tokotype](https://github.com/tokotype/PlusJakartaSans) | SIL OFL 1.1; `assets/fonts/OFL.txt` |
| Poppins | [Indian Type Foundry](https://github.com/itfoundry/Poppins) | SIL OFL 1.1; `assets/fonts/Poppins-OFL.txt` |

`assets/figma` contains the project's supplied interface exports and PNG
conversions. Third-party icon shapes retain upstream rights; Phosphor's notice
is included with the icon bundle. Spotify and other brand names or marks belong
to their respective owners; inclusion does not imply endorsement or grant
trademark rights. SF Pro is not bundled.

The fixture cover in `assets/fixtures` is synthetic test artwork. Actual music
covers and lyrics are runtime content, not repository or release assets.

Rust dependency versions are recorded in `Cargo.lock`; their licenses remain
with the upstream packages. Optional PNG conversion uses `@resvg/resvg-js`
(MPL-2.0), which is not included in the application runtime or release bundle.

Lyricsfile parsing uses `serde_yaml_ng` (MIT OR Apache-2.0) and its dependencies;
see the exact versions in `Cargo.lock`. Lyrics and their timing data remain
runtime content supplied by the lyric provider.

Release packages include this file, the application license, icon license and
font licenses. Review dependency terms when changing dependencies/distribution.
