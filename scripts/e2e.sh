#!/usr/bin/env bash
# End-to-end check: a real server, a simulated Modbus TCP device, and the workbench driven in a
# real headless browser. It adds a connection through the form, has the controller refuse a bad
# one, saves a good one, and reads live values off the page.
#
# This is not part of scripts/check.sh and does not run in CI: it needs Chromium. If it cannot
# run it says so and exits non-zero. It never reports a pass for a check that did not execute.
#
# Usage: ./scripts/e2e.sh
#   MYTHOS_E2E_HTTP_PORT  server port          (default 18084)
#   MYTHOS_E2E_SIM_PORT   simulated device port (default 1504)
#   CHROMIUM              browser binary        (default: chromium, then chromium-browser, then google-chrome)
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HTTP_PORT="${MYTHOS_E2E_HTTP_PORT:-18084}"
SIM_PORT="${MYTHOS_E2E_SIM_PORT:-1504}"

cannot_run() {
    printf 'FAIL e2e (not run): %s\n' "$1"
    exit 1
}

BROWSER="${CHROMIUM:-}"
if [ -z "$BROWSER" ]; then
    for candidate in chromium chromium-browser google-chrome; do
        if command -v "$candidate" >/dev/null; then BROWSER="$candidate"; break; fi
    done
fi
[ -n "$BROWSER" ] || cannot_run "no Chromium found; set CHROMIUM to a browser binary"
command -v "$BROWSER" >/dev/null || cannot_run "browser '$BROWSER' is not an executable on PATH"
command -v node >/dev/null || cannot_run "node is not installed"
node -e 'process.exit(typeof WebSocket === "function" ? 0 : 1)' \
    || cannot_run "node $(node --version) has no global WebSocket; Node 22 or newer is needed"
command -v cargo >/dev/null || cannot_run "cargo is not installed"
[ -d "$ROOT/frontend/node_modules" ] \
    || cannot_run "frontend/node_modules missing: run pnpm install --frozen-lockfile"

# Both ports must be free before anything starts. If one is taken, the readiness probe below
# would be answered by whatever holds it, and this run would go on to create a project on a
# server that is not its own.
port_in_use() { (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null; }
port_in_use "$HTTP_PORT" && cannot_run "port $HTTP_PORT is already in use; set MYTHOS_E2E_HTTP_PORT"
port_in_use "$SIM_PORT" && cannot_run "port $SIM_PORT is already in use; set MYTHOS_E2E_SIM_PORT"

echo "=== build ==="
( cd "$ROOT" && cargo build --bin mythos-server \
    && cargo build -p driver-modbus --bin mythos-module-modbus \
    && cargo build -p driver-modbus --features sim --example modbus_sim ) \
    || cannot_run "the server or the simulator did not build"
( cd "$ROOT/frontend" && pnpm build ) >/dev/null || cannot_run "the workbench did not build"

WORK="$(mktemp -d -t mythos-e2e-XXXXXX)"
SHM="mythos_tagbus_e2e_$$"
SRV=""; SIM=""
cleanup() {
    [ -n "$SRV" ] && kill "$SRV" 2>/dev/null
    [ -n "$SIM" ] && kill "$SIM" 2>/dev/null
    wait 2>/dev/null
    rm -f "/dev/shm/$SHM"
}
trap cleanup EXIT

mkdir -p "$WORK/data" "$WORK/shots"
API="http://127.0.0.1:${HTTP_PORT}"

MYTHOS_STORAGE_ROOT="$WORK/data" MYTHOS_HTTP_PORT="$HTTP_PORT" MYTHOS_SHM_NAME="$SHM" \
MYTHOS_STATIC_DIR="$ROOT/frontend/packages/ui-shell/dist" \
    "$ROOT/target/debug/mythos-server" >"$WORK/server.log" 2>&1 & SRV=$!
"$ROOT/target/debug/examples/modbus_sim" "127.0.0.1:${SIM_PORT}" >"$WORK/sim.log" 2>&1 & SIM=$!

ready=""
for _ in $(seq 1 50); do
    # The process must still be ours: if the port was taken, our server has exited and whatever
    # answers on it is some other server, which this run must not go on to test.
    kill -0 "$SRV" 2>/dev/null \
        || cannot_run "the server exited at start; is port $HTTP_PORT in use? (see $WORK/server.log)"
    if curl -fs -m 1 -o /dev/null "$API/api/v1/health"; then ready=1; break; fi
    sleep 0.2
done
[ -n "$ready" ] || cannot_run "the server did not answer on $API (see $WORK/server.log)"
kill -0 "$SRV" 2>/dev/null \
    || cannot_run "the server exited at start; is port $HTTP_PORT in use? (see $WORK/server.log)"
kill -0 "$SIM" 2>/dev/null || cannot_run "the simulator did not start on port $SIM_PORT (see $WORK/sim.log)"

# The server under test was started on an empty storage root. If the one answering has projects,
# it is not ours, and nothing may be written to it.
[ "$(curl -fsS -m 3 "$API/api/v1/projects")" = "[]" ] \
    || cannot_run "the server on $API is not the one this run started (its storage is not empty)"

# Field I/O runs through driver modules. The server must have found the Modbus one beside it,
# as a module process and not anything compiled in.
curl -fsS -m 3 "$API/api/v1/modules" | node -e '
    let s = ""; process.stdin.on("data", d => s += d).on("end", () => {
        const m = JSON.parse(s).drivers.find(d => d.name === "modbus");
        process.exit(m && m.source === "module" && m.module.manifest.id === "org.mythos.driver.modbus" ? 0 : 1);
    });' || cannot_run "the server did not find the Modbus driver module (see $WORK/server.log)"

curl -fsS -o /dev/null -X POST "$API/api/v1/projects" -H 'content-type: application/json' \
    -d '{"id":"plant_a","name":"Boiler House","description":"Steam plant","author":"e2e"}' \
    || cannot_run "the test project could not be created"
[ -d "$WORK/data/plant_a" ] \
    || cannot_run "the test project was not created in this run's storage ($WORK/data)"
curl -fsS -o /dev/null -X POST "$API/api/v1/projects/plant_a/activate" \
    || cannot_run "the test project could not be activated"

echo "=== workbench, in $BROWSER ==="
node "$ROOT/frontend/e2e/workbench.mjs" "$WORK/shots" "$API" "$SIM_PORT" "$BROWSER"
status=$?

echo
echo "screenshots and logs: $WORK"
if [ "$status" -ne 0 ]; then
    echo "VERDICT: FAIL"
    exit 1
fi
echo "VERDICT: PASS"
