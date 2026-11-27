# Security

## Reporting a vulnerability

Please report security problems privately through GitHub's
[private vulnerability reporting](https://github.com/Loopback-Games/scrapburn/security/advisories/new),
not in a public issue. Include what you found, how to reproduce it, and the version
(the `scrapburn.release` file in your install folder) or commit it affects.

## Supported versions

Only the latest release is supported. The game offers to install it from the title
screen.

## How releases reach players

- Releases are built by `.github/workflows/release.yml` from a tag on `main`, with a
  `SHA256SUMS` file and a GitHub build-provenance attestation for every archive.
- The in-game updater fetches the latest release over HTTPS (OS trust store), checks
  the archive against `SHA256SUMS` and GitHub's own asset digest, and refuses archive
  entries that would land outside the install folder. Verify an archive by hand with
  `gh attestation verify <archive> --repo Loopback-Games/scrapburn`.
- Set `SCRAPBURN_NO_UPDATE=1` to switch the updater off.
