#!/usr/bin/env python3
"""Install a release bundle the way an operator would, then run what it installed.

1. Serve the bundle and its SHA256SUMS from a local release feed shaped like GitHub's.
2. `mythos update --version <v>` with the CLI from the bundle: download, checksum, unpack,
   run check, switch `current`.
3. Start `current/bin/mythos-server` from the install, on a free port with its own storage.
4. Create its first administrator with the setup code this script gave it, sign in, and
   require that it found the Modbus module beside itself (as a module process), that it knows
   the installation it runs from (Settings → Updates), and that it serves the workbench from
   the bundle's web/.

Exits non-zero, saying which step failed, otherwise.

Usage:
    release-smoke.py --bundle dist/mythos-1.2.0-<target>.zip --version 1.2.0
"""

import argparse
import hashlib
import http.cookiejar
import http.server
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
import zipfile


def fail(step: str, detail: str) -> int:
    print(f"release-smoke: FAILED at {step}: {detail}", file=sys.stderr)
    return 1


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle", required=True)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    version = args.version.lstrip("v")
    exe = ".exe" if os.name == "nt" else ""

    work = tempfile.mkdtemp(prefix="mythos-smoke-")
    feed_dir = os.path.join(work, "feed")
    os.makedirs(feed_dir)
    name = os.path.basename(args.bundle)
    shutil.copy(args.bundle, os.path.join(feed_dir, name))
    with open(args.bundle, "rb") as f:
        digest = hashlib.sha256(f.read()).hexdigest()
    with open(os.path.join(feed_dir, "SHA256SUMS"), "w") as f:
        f.write(f"{digest}  {name}\n")

    port = free_port()
    base = f"http://127.0.0.1:{port}"
    with open(os.path.join(feed_dir, "releases.json"), "w") as f:
        json.dump(
            [
                {
                    "tag_name": f"v{version}",
                    "draft": False,
                    "assets": [
                        {"name": name, "browser_download_url": f"{base}/{name}"},
                        {"name": "SHA256SUMS", "browser_download_url": f"{base}/SHA256SUMS"},
                    ],
                }
            ],
            f,
        )

    class Quiet(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=feed_dir, **kw)

        def log_message(self, *a):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", port), Quiet)
    threading.Thread(target=server.serve_forever, daemon=True).start()

    # The CLI that does the installing comes out of the bundle itself.
    unpacked = os.path.join(work, "unpacked")
    with zipfile.ZipFile(args.bundle) as zf:
        zf.extractall(unpacked)
    cli = os.path.join(unpacked, "bin", "mythos" + exe)
    if os.name != "nt":
        os.chmod(cli, 0o755)

    home = os.path.join(work, "home")
    env = dict(os.environ, MYTHOS_RELEASE_FEED=f"{base}/releases.json")
    run = subprocess.run(
        [cli, f"--mythos-dir={home}", "update", "--version", version],
        env=env,
        capture_output=True,
        text=True,
        timeout=300,
    )
    print(run.stdout.strip())
    if run.returncode != 0:
        return fail("mythos update", run.stderr.strip() or run.stdout.strip())

    current_bin = os.path.join(home, "current", "bin")
    server_exe = os.path.join(current_bin, "mythos-server" + exe)
    if not os.path.isfile(server_exe):
        return fail("install layout", f"{server_exe} is missing")

    http_port = free_port()
    storage = os.path.join(work, "storage")
    setup_code = f"smoke-{os.getpid()}-{digest[:12]}"
    server_env = dict(
        os.environ,
        MYTHOS_SETUP_CODE=setup_code,
        MYTHOS_STORAGE_ROOT=storage,
        MYTHOS_HTTP_PORT=str(http_port),
        MYTHOS_SHM_NAME=f"mythos_smoke_{os.getpid()}",
    )
    server_env.pop("MYTHOS_STATIC_DIR", None)
    server_env.pop("MYTHOS_MODULE_PATH", None)
    server_env.pop("MYTHOS_HOME", None)
    log_path = os.path.join(work, "server.log")
    log = open(log_path, "w")
    proc = subprocess.Popen([server_exe], env=server_env, stdout=log, stderr=subprocess.STDOUT)
    try:
        api = f"http://127.0.0.1:{http_port}"
        up = False
        deadline = time.time() + 60
        while time.time() < deadline:
            if proc.poll() is not None:
                break
            try:
                with urllib.request.urlopen(f"{api}/api/v1/health", timeout=2):
                    up = True
                break
            except OSError:
                time.sleep(0.25)
        if not up:
            log.flush()
            return fail("server start", open(log_path).read()[-4000:])

        # Every route needs an account: create the first administrator, signed in by a cookie.
        jar = http.cookiejar.CookieJar()
        opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
        setup = urllib.request.Request(
            f"{api}/api/v1/auth/setup",
            data=json.dumps(
                {"setup_code": setup_code, "username": "smoke", "password": "smoke password 1"}
            ).encode(),
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            opener.open(setup, timeout=30).read()
        except OSError as e:
            return fail("first administrator", str(e))

        def get(path):
            with opener.open(f"{api}{path}", timeout=10) as r:
                return json.load(r)

        modules = get("/api/v1/modules")

        drivers = {d.get("name"): d.get("source") for d in modules.get("drivers", [])}
        if drivers.get("modbus") != "module":
            return fail("module discovery", json.dumps(modules)[:2000])

        # The HMI is a service module with a workbench UI (ADR-011): the installed server must
        # run it and serve its UI from the bundle's modules-ui/.
        hmi = next((m for m in modules.get("modules", []) if m.get("id") == "org.mythos.hmi"), None)
        if not hmi or not hmi.get("enabled") or not hmi.get("ui"):
            return fail("HMI module", json.dumps(modules)[:2000])
        try:
            with opener.open(f"{api}{hmi['ui']}", timeout=10) as r:
                entry = r.read()
        except OSError as e:
            return fail("HMI module UI", str(e))
        if b"org.mythos.hmi" not in entry:
            return fail("HMI module UI", entry[:300].decode("utf-8", "replace"))

        # Discovery (ADR-012): always on, told its node; an installed edge announces itself.
        node = get("/api/v1/node")
        heard = None
        deadline = time.time() + 30
        while time.time() < deadline:
            heard = get("/api/v1/m/discovery/nodes")
            if heard.get("self") == node.get("id"):
                break
            time.sleep(0.5)
        if not heard or heard.get("self") != node.get("id") or not heard.get("listening"):
            return fail("discovery module", json.dumps(heard)[:2000])

        updates = get("/api/v1/updates")
        installation = updates.get("installation") or {}
        if installation.get("current_version") != version or updates["running"].get("installed_version") != version:
            return fail("installation", json.dumps(updates)[:2000])

        with urllib.request.urlopen(f"{api}/", timeout=5) as r:
            page = r.read().decode("utf-8", "replace")
        if "<html" not in page.lower():
            return fail("workbench", page[:500])

        print(
            f"release-smoke: OK — installed {version}; the server found the Modbus and HMI modules, serves "
            f"the HMI's UI, discovery knows the node, knows it "
            f"runs {version} from {installation.get('home')}, and serves the workbench"
        )
        return 0
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
        log.close()
        server.shutdown()


if __name__ == "__main__":
    sys.exit(main())
