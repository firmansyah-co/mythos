# Getting started

Build MythOS from source, start a controller, and sign in for the first time. Back to the [README](../../README.md).

## Building from source

Requirements: Linux, macOS or Windows; Rust 1.82 or newer, Node.js 22 and pnpm. On Windows the
executables end in `.exe` (`target\debug\mythos-server.exe`) and the variables below are set with
`$env:NAME = "value"` in PowerShell.

```bash
git clone https://github.com/firmansyah-co/mythos.git
cd mythos

cargo build -p mythos-core -p driver-modbus --bin mythos-server --bin mythos-module-modbus

cd frontend
pnpm install --frozen-lockfile
pnpm build
cd ..
```

Run the server and open `http://localhost:8080`:

```bash
MYTHOS_STORAGE_ROOT="$PWD/.mythos-data" \
MYTHOS_STATIC_DIR="$PWD/frontend/packages/ui-shell/dist" \
  ./target/debug/mythos-server
```

| Variable | Default | Meaning |
| :--- | :--- | :--- |
| `MYTHOS_STORAGE_ROOT` | Linux: `/var/lib/mythos/projects`; macOS: `/Library/Application Support/MythOS/projects`; Windows: `%ProgramData%\MythOS\projects` | Where projects are stored. |
| `MYTHOS_HTTP_PORT` | `8080` | Listening port. |
| `MYTHOS_STATIC_DIR` | unset | Directory of the built web workbench to serve. |
| `MYTHOS_SHM_NAME` | `mythos_tagbus` | Name of the tag bus segment. Two servers must not share one: the tag bus has a single writer. |
| `MYTHOS_MODULE_PATH` | the server's own directory | Directories to look for module executables in, separated like `PATH`. |

`GET /api/v1/modules` lists the modules the server found, the schema of the configuration each
accepts, and any executable named like a module that could not be used, with the reason.
`POST /api/v1/modules/rescan` looks again at once; the server also does that by itself when a
module executable is added, removed or replaced.

To replace a module, put the new build beside it under another name and rename it over the old
one. Windows does not let a running executable be replaced: rename the old build away first (to
any name that is not a module's, such as `mythos-module-modbus.old`), then move the new one in.
The connections using the module keep running while it is missing.

## First start

A controller starts with no account. Its log says:

```text
No account exists yet. Open the workbench and create the first administrator with setup code …
```

Open the workbench, enter that code, and choose the administrator's username and password.
Further accounts are made in Settings → Users. `MYTHOS_SETUP_CODE` sets the code instead of a
random one, for automated installs.

## Verifying a change

```bash
./scripts/check.sh    # cargo fmt, clippy, test; pnpm lint, test, build
./scripts/e2e.sh      # a real server, a simulated device, and the workbench in a real browser
```

`e2e.sh` needs Chromium and is not run in CI. It adds a connection through the workbench's form,
has the controller refuse a bad one, saves a good one, reads live values off the page, then
draws an HMI screen, binds a field to a tag, drags an element, saves and releases it, and
opens the operator runtime on it. It also compiles a small control program
(`frontend/e2e/fan.st`), loads it into the VM runtime over the API, and on Settings → Runtimes
sees it run, takes the browser off the network to see the page mark it as the last known
state, stops it and sees its output confirmed at its safe value, and runs it again.

See [`CLAUDE.md`](../../CLAUDE.md) and [`CONTRIBUTING.md`](../../CONTRIBUTING.md) for the working rules.
