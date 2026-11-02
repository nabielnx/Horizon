# Initial repository audit — 2026-11-02

Scope: local source, assets, scripts, manifests, build configuration and candidate
commit files. No external source scanner was used. This is a repository-readiness
check, not a security certification.

| Destination | Files |
| --- | --- |
| Git repository | Rust source, Cargo manifest/lockfile, maintained scripts, synthetic fixture, required UI/font/icon assets with notices, documentation and CI |
| Build artifacts / GitHub Releases | Executable, fonts, licenses, documentation, ZIP and SHA-256 checksum |
| Local only | `target`, `artifacts`, local toolchain/shims in `tools`, `.local-build.json`, machine-specific Cargo config, old icon backups, logs, generated probes/captures and runtime data |

## Findings addressed

- Ignore rules excluded outputs/configuration but missed old icon backups. Rules
  now cover those, dependencies, credentials, signing material and runtime data.
- Check/test/format scripts required private machine configuration. A shared
  helper now supports normal Cargo and project-relative paths.
- Asset conversion depended on ignored `artifacts/design-tools/node_modules`.
  It now has pinned dependencies and an npm lockfile under `scripts`.
- Active SVG icons are Phosphor, while the old notice identified Lucide. The
  active notice/package attribution now match Phosphor. The old icon backup stays
  local. Font licenses are preserved.
- The package declared MIT without a root application license. Application and
  third-party notices are now present.
- Packaging used hardcoded version paths. It now reads the Cargo version and
  generates a ZIP/checksum; binaries remain outside source control.
- The README mixed outdated behavior and local debug references. Usage,
  development, architecture and privacy documentation are now separate.
- Windows CI checks formatting, lint, tests, packaging and native offscreen
  probes. External actions are pinned to verified commits and use read-only
  repository permissions.

## Remaining limits and follow-up

- Observe the first remote CI result: local GNU success alone does not establish
  MSVC or fresh-runner compatibility.
- Rust follows stable rather than an exact compiler pin. Lockfiles are committed;
  fully reproducible binaries have not been demonstrated.
- Offscreen probes cannot validate live media, lyric-service uptime, startup
  after reboot or visible compositor behavior.
- Review attribution when adding exports or brand assets. This repository does
  not grant third-party trademark or song rights.
- Enable private vulnerability reporting and suitable branch protection when
  account settings permit. Policy documents alone do not enable either.
- Refactor large runtime/rendering modules and retire old pixel baselines in
  focused changes after the initial import.

A limited text scan found no obvious hardcoded credential patterns in candidate
source/assets/scripts. Review the staged inventory before pushing; ignore rules
do not replace review of future commits.

## Local validation

- Formatting and Clippy passed.
- Unit tests: 53 passed, 0 failed, 1 opt-in live network test ignored.
- Release executable, ZIP, font/icon notices and SHA-256 file generated.
- Native offscreen startup, settings/layout and 120-frame motion probes passed.
- All 24 SVG exports rendered in memory using the declared tooling dependency.
- PowerShell scripts parsed successfully; excluded paths were checked against
  the Git ignore rules.

Windows file-replacement tests required running outside the execution sandbox;
their first sandboxed attempt was denied by the host's filesystem restrictions.
No desktop screenshots were required for this verification.
