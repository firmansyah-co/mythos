# Installing and updating

How release bundles are installed, updated, pinned and rolled back. **No release has been published yet**, so `mythos update` has nothing to install and says so. Back to the [README](../../README.md).

A release publishes one bundle per platform, `mythos-<version>-<target>.zip`, and a
`SHA256SUMS` file. The bundles are built for Linux (x86-64 and arm64), macOS (Apple silicon) and
Windows (x86-64) by `.github/workflows/release-assets.yml`, which also installs each one with
its own CLI and starts its server before attaching it to the release. A bundle holds `bin/`
(`mythos`, `mythos-server`, the driver modules) and `web/` (the workbench, which the server
serves from there when `MYTHOS_STATIC_DIR` is not set).

```bash
mythos update                    # the pinned version, else the newest on the channel
mythos update --version 1.2.0    # a given version, once
mythos pin 1.2.0                 # stay on 1.2.0 (mythos unpin to follow the channel again)
mythos versions --remote         # what has been published, and whether this platform has a bundle
mythos versions                  # what is installed
mythos rollback                  # back to the previous version, which is still installed
mythos channel switch rc         # follow release candidates too (or dev for everything)
```

`update` reads GitHub's release list for this repository (`MYTHOS_RELEASE_FEED` or
`remote_url` in `~/.mythos/config.json` point it elsewhere). It refuses a bundle whose SHA-256
does not match `SHA256SUMS`, an archive with an entry outside its directory, and a bundle whose
`mythos` does not run on this machine; only then does it move `~/.mythos/current` (a symlink,
or a junction on Windows) to `~/.mythos/versions/<version>`. Put `~/.mythos/current/bin` on
`PATH`. No executable is ever overwritten, so updating while the server runs is safe; the
running server keeps its version until it is restarted from `current`. The checksum guards
against a damaged download, not against a compromised release: bundles are not signed yet.

The same is in the workbench, Settings → Updates: the version the server runs and the
installation it runs from (`<home>/versions/<version>/bin`, or `MYTHOS_HOME`), every published
version with its channel, date and release notes, and — for an administrator — following a
channel or staying on one version, installing, and rolling back. A server built from source runs
from no installation; it lists the releases and says it has nowhere to install them.
