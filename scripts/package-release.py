#!/usr/bin/env python3
"""Pack one platform's release bundle: mythos-<version>-<target>.zip.

    bin/mythos[.exe]                 the CLI (update, rollback, plugins)
    bin/mythos-server[.exe]          the controller
    bin/mythos-module-modbus[.exe]   driver modules, found beside the server
    web/                             the workbench, served by the server from ../web

The layout is what `mythos update` installs and what mythos-server looks for, so changing it
means changing crates/mythos-cli/src/install.rs and crates/mythos-core/src/bin/mythos-server.rs.

Usage:
    package-release.py --version 1.2.0 --target x86_64-unknown-linux-gnu \\
        --bin-dir target/release --web frontend/packages/ui-shell/dist --out dist
"""

import argparse
import os
import sys
import zipfile

EXECUTABLES = ["mythos", "mythos-server", "mythos-module-modbus"]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True, help="the release version, without a leading v")
    parser.add_argument("--target", required=True, help="the Rust target triple built for")
    parser.add_argument("--bin-dir", required=True, help="where cargo put the executables")
    parser.add_argument("--web", required=True, help="the built workbench (holds index.html)")
    parser.add_argument("--out", required=True, help="directory to write the bundle into")
    args = parser.parse_args()

    version = args.version.lstrip("v")
    suffix = ".exe" if "windows" in args.target else ""
    missing = []
    for name in EXECUTABLES:
        if not os.path.isfile(os.path.join(args.bin_dir, name + suffix)):
            missing.append(name + suffix)
    if not os.path.isfile(os.path.join(args.web, "index.html")):
        missing.append(os.path.join(args.web, "index.html"))
    if missing:
        print(f"package-release: missing {', '.join(missing)}", file=sys.stderr)
        return 1

    os.makedirs(args.out, exist_ok=True)
    bundle = os.path.join(args.out, f"mythos-{version}-{args.target}.zip")
    with zipfile.ZipFile(bundle, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for name in EXECUTABLES:
            info = zipfile.ZipInfo(f"bin/{name}{suffix}")
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (0o100755 << 16)  # a regular, executable file
            with open(os.path.join(args.bin_dir, name + suffix), "rb") as f:
                zf.writestr(info, f.read())
        for root, _dirs, files in os.walk(args.web):
            for file in sorted(files):
                path = os.path.join(root, file)
                relative = os.path.relpath(path, args.web).replace(os.sep, "/")
                info = zipfile.ZipInfo(f"web/{relative}")
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = (0o100644 << 16)
                with open(path, "rb") as f:
                    zf.writestr(info, f.read())
    print(bundle)
    return 0


if __name__ == "__main__":
    sys.exit(main())
