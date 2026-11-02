# Security policy

Horizon is pre-1.0 software. Security fixes target current source on `main`;
there is no commitment to backport fixes to older binaries.

Use GitHub's **Report a vulnerability** option if available for this repository.
Do not post credentials, personal application data, or exploit details publicly.
If private reporting is unavailable, open an issue requesting a private contact
method without including sensitive details.

Reports should identify the version, impact and minimal reproduction using
synthetic data. Distinguish application flaws from external media/lyric-service
failures.

Release executables are currently unsigned. SHA-256 checksums detect download
changes; they do not replace code signing. Download packages only from this
repository's own Actions artifacts or Releases.
