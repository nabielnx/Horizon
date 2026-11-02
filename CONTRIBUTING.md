# Contributing

Horizon is a native Windows application. Read [development](docs/development.md)
and [architecture](docs/architecture.md) before changing runtime or rendering code.

1. Open an issue for substantial behavior or design changes.
2. Work on a focused branch and explain the visible problem in the pull request.
3. Run `./build.ps1` and `./scripts/verify.ps1 -SkipPreviews` on Windows.
4. Include tests for behavior changes and describe manual validation for compositor
   behavior, real media sessions, startup, or notifications.

Keep generated probes, local data and screenshots out of commits. Use synthetic
fixtures when demonstrating issues. Do not share credentials, personal paths,
calendar entries, tasks or lyrics caches. See [SECURITY.md](SECURITY.md) for
vulnerability reports.

Keep Win32/COM lifetime changes and `unsafe` blocks small; document ownership and
thread assumptions. Preserve data compatibility. Performance claims need a
description of how they were measured.

Application contributions use the project's MIT license. Third-party assets must
include their original license and attribution.
