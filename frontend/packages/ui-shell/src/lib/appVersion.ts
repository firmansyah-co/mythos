// Version identity of the running build.
//
// The value comes from this package's manifest, which scripts/version.sh stamps from the release
// tag. It is imported rather than written as a literal: a hardcoded version is how this product
// ended up with no version anywhere, and an injected global can silently go missing (a build that
// skipped the injection would report a plausible-looking number rather than an absent one).
//
// Reading the manifest keeps dev, test and production builds on one source, so the version the
// operator sees is the version that was built.

import pkg from '../../package.json';

export const APP_VERSION: string = pkg.version;
