# MythOS Architecture Specification: One-Click Desktop Installers & Universal Distribution

- **Document Identifier**: MYTHOS-ARCH-008
- **Version**: 1.2.0
- **Author**: Neo (VP Software & Infrastructure), Scribe (VP Publication & Documentation)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification establishes the official **One-Click Distribution Strategy** for MythOS.

In response to direct mandates from Adrian (Board of Commissioners), installing MythOS must be as seamless and familiar as installing Siemens WinCC or modern desktop developer applications (e.g., VSCode, Docker Desktop, Obsidian). Users on **Windows**, **macOS**, and **Linux** must not be forced to install compilers, configure shell scripts, or compile code manually from source to launch the engineering studio.

Furthermore, field engineers, Industrial PC (IPC) operators, and headless edge controllers require automated bootstrap scripts with multi-channel release feeds, SemVer version pinning, embedded Web HMI interfaces, and instant local rollback capabilities.

---

## 2. Universal Distribution Artifacts & Multi-Channel Feed

MythOS automated CI/CD pipelines compile and package official production installers, CLI binaries, and bootstrap scripts for every supported platform on each release.

### 2.1 Distribution Endpoints
- **Universal Bootstrap URL**: `https://get.mythos.firmansyah.co`
  * Serves the Linux/macOS POSIX shell bootstrap script at root (`/`).
  * Serves the Windows PowerShell bootstrap script at `/win`.
- **Release Feed & Manifest CDN**: `https://releases.mythos.firmansyah.co`
  * Serves channel manifests, checksum registries, and version index feeds.

### 2.2 Distribution Channels
The distribution infrastructure provides three distinct release channels:

| Channel | Identifier | Stability Gate | Update Frequency | Target Audience |
| :--- | :--- | :--- | :--- | :--- |
| **Release** | `release` | Strict Production (All CI tests, Box-Model audits & OT sign-offs) | Bi-weekly / Monthly | Production factory floors, production SCADA workstations |
| **Release Candidate** | `rc` | Staging Release (All automated CI and integration tests passed) | Weekly | Pre-production testing, QA engineers, integration staging |
| **Development** | `dev` | Bleeding edge (Automated continuous build from `dev` branch) | Continuous / Nightly | Field commissioning engineers, active developers |

```
+----------------------------------------------------------------------------------------------------------------+
|                                    MYTHOS UNIVERSAL DISTRIBUTION SUITE                                         |
|                                  Official CDN: get.mythos.firmansyah.co                                        |
|                                                                                                                |
|  [ LINUX / macOS BASH ]        [ WINDOWS POWERSHELL ]        [ DESKTOP BUNDLES ]       [ HEADLESS CONTROLLER ] |
|  curl | bash Installer         irm /win | iex                Standalone GUI Bundles    1-Command systemd setup |
|  • Multi-channel (--channel)   • Multi-channel (-Channel)    • NSIS Setup (.exe)       • Architecture auto-det |
|  • Version pin (--version)     • Version pin (-Version)      • Universal DMG (.dmg)    • RT caps (CAP_SYS_NICE)|
|  • Auto launcher (.desktop)    • Start Menu shortcuts        • Universal AppImage      • /var/lib/mythos/ prov |
|  • Instant local rollback      • Instant local rollback      • Native .deb / .rpm      • Auto-run on boot      |
+----------------------------------------------------------------------------------------------------------------+
                                           |
                                           v
                   +------------------------------------------------+
                   |           ALL-IN-ONE HYBRID ARCHITECTURE       |
                   |  1. Standalone Microkernel Daemon              |
                   |  2. Embedded Web HMI Studio (http://localhost:8080) |
                   |  3. First-Class CLI Utility (mythos-cli)       |
                   +------------------------------------------------+
```

---

## 3. Platform Packaging Specifications

### 3.1 Linux & macOS: Universal POSIX Bootstrap Script
- **Installation Endpoint**: `https://get.mythos.firmansyah.co`
- **Default Execution**:
  ```bash
  curl -fsSL https://get.mythos.firmansyah.co | bash
  ```
- **Execution Options**:
  * Flag syntax:
    ```bash
    curl -fsSL https://get.mythos.firmansyah.co | bash -s -- --channel rc --version 1.2.0-rc.1
    ```
  * Environment variable syntax:
    ```bash
    CHANNEL=dev VERSION=latest curl -fsSL https://get.mythos.firmansyah.co | bash
    ```
- **Automated Workflow**:
  1. Detects operating system (`linux`, `darwin`) and CPU architecture (`x86_64`, `aarch64`, `armv7l`).
  2. Queries `https://releases.mythos.firmansyah.co/manifests/<channel>.json` for the target version binary.
  3. Downloads release archive containing `mythos-server` and `mythos` CLI binaries.
  4. Verifies SHA-256 cryptographic hash against the official release manifest.
  5. Installs binaries into `/usr/local/bin/` (with `sudo`) or `~/.mythos/bin/` (unprivileged).
  6. Configures user `PATH` environment variable if required.
  7. Registers desktop application launcher:
     - Linux: Installs `~/.local/share/applications/mythos.desktop` with high-resolution SVG icon.
     - macOS: Creates `/Applications/MythOS.app` application wrapper pointing to the embedded GUI.

### 3.2 Windows 10 & 11: Universal PowerShell Bootstrap Script
- **Installation Endpoint**: `https://get.mythos.firmansyah.co/win`
- **Default Execution**:
  ```powershell
  irm https://get.mythos.firmansyah.co/win | iex
  ```
- **Execution Options**:
  ```powershell
  & ([scriptblock]::Create((irm https://get.mythos.firmansyah.co/win))) -Channel rc -Version 1.2.0-rc.1
  ```
- **Automated Workflow**:
  1. Verifies PowerShell version (PowerShell 5.1+ or PowerShell Core 7+).
  2. Detects architecture (`x64` or `arm64`).
  3. Downloads matching binary release bundle from official CDN.
  4. Validates SHA-256 cryptographic checksum.
  5. Installs binaries into `%LOCALAPPDATA%\MythOS\bin` or `C:\Program Files\MythOS`.
  6. Appends installation directory to persistent user `PATH`.
  7. Generates Windows Start Menu and Desktop shortcuts to launch MythOS.

### 3.3 Standalone Graphical Installers (Zero-CLI Users)
For operators and factory personnel requiring standard graphical wizards:
- **Windows Setup (`MythOS-Setup-x64.exe`)**:
  * Packaging Engine: NSIS / WiX Toolset via Tauri v2.
  * Standard installation wizard with license acceptance, target directory selection, and optional background service setup.
  * Silent Enterprise Deployment:
    ```powershell
    MythOS-Setup-x64.exe /S /D=C:\Program Files\MythOS
    ```
- **macOS Universal Disk Image (`MythOS-Universal.dmg`)**:
  * Packaging Engine: Apple `.dmg` containing Universal 2 binary (`x86_64` + `arm64`).
  * User drags MythOS icon into `/Applications`.
  * Fully signed with Apple Developer ID and notarized by Apple Ticket Service.
  * Uses native macOS `WKWebView` with zero Chromium process overhead.
- **Linux Standalone AppImage & Packages**:
  * Universal AppImage (`MythOS-x86_64.AppImage`): Self-contained executable running across all major distributions.
  * Distribution packages: Debian/Ubuntu (`.deb`), Fedora/RHEL (`.rpm`), and Arch Linux (`AUR`).

### 3.4 Headless Industrial IPC & Edge Controllers
For industrial field controllers (Raspberry Pi Compute Module 4, Advantech, Siemens IPC) without displays:
- **Execution**:
  ```bash
  curl -fsSL https://get.mythos.firmansyah.co | sudo bash
  ```
- **Automated Workflow**:
  1. Detects system architecture (`x86_64`, `aarch64`, `armv7l`).
  2. Sets up runtime directories at `/var/lib/mythos/` and configuration at `/etc/mythos/`.
  3. Creates dedicated system user `mythos` with `uucp` / `dialout` serial port permissions.
  4. Grants POSIX capabilities for real-time operation (`CAP_SYS_NICE`, `CAP_NET_RAW`).
  5. Registers and starts `systemd` unit `mythos.service`.
  6. Displays confirmation message indicating active listener at `http://<device-ip>:8080`.

### 3.5 Embedded Web HMI GUI Architecture
All distributed microkernel daemon binaries bundle static assets for the complete MythOS Web Engineering Studio:
- **Zero External Web Server**: The microkernel serves the Web HMI directly via its internal Axum HTTP engine on `http://localhost:8080`.
- **Offline Self-Contained**: The embedded assets require zero internet access during plant floor operation.
- **Desktop Launcher Interoperability**: Desktop shortcuts launch the default system browser or lightweight webview pointed to `http://localhost:8080`.

---

## 4. CLI Lifecycle Management & Instant Rollback Architecture

The `mythos` command-line interface (`crates/mythos-cli`) provides built-in lifecycle management, channel navigation, and instant recovery.

### 4.1 CLI Lifecycle Subcommands
The `mythos` executable provides the following management subcommands:

```bash
# Display active channel and switch channels
mythos channel
mythos channel switch <release|rc|dev>

# Query remote release versions from CDN
mythos versions --remote

# Update to latest channel version or pin to explicit SemVer
mythos update
mythos update --version 1.2.0

# Instant rollback to previously cached working version
mythos rollback
```

### 4.2 Local Version Cache & Atomic Symlink Mechanism
To guarantee safety and continuous plant uptime, the installer maintains a local version cache directory:

```
~/.mythos/
├── bin/
│   ├── mythos-server -> ../versions/1.2.0/mythos-server   (Active symlink or hardlink)
│   └── mythos        -> ../versions/1.2.0/mythos
├── current.json                                          (Active release state)
├── previous.json                                         (Previous functional state)
└── versions/
    ├── 1.1.4/
    │   ├── mythos-server
    │   └── mythos
    └── 1.2.0/
        ├── mythos-server
        └── mythos
```

- **Update Workflow**:
  1. The CLI queries `https://releases.mythos.firmansyah.co` for version manifests.
  2. The update downloads the target payload into a new directory under `~/.mythos/versions/<version>/`.
  3. The CLI verifies SHA-256 cryptographic signatures.
  4. The CLI atomically swaps the binary symlinks or pointer file.
  5. The previous version remains preserved in `~/.mythos/versions/<previous-version>/`.
  6. The CLI logs state into `~/.mythos/current.json` and updates `~/.mythos/previous.json`.

- **Instant Rollback Workflow (`mythos rollback`)**:
  1. Operator issues `mythos rollback`.
  2. The CLI reads the previous version tag from `~/.mythos/previous.json`.
  3. The CLI checks existence of the cached binary directory.
  4. The CLI atomically swaps active symlinks or executables back to the cached version.
  5. The entire rollback completes in $< 1.0\,\text{s}$ without requiring network access or re-downloading.
  6. If registered with `systemd`, the CLI invokes `systemctl restart mythos` to reload the verified microkernel.

### 4.3 In-App Auto-Update Integration
For graphical desktop users:
1. The desktop application periodically checks `https://releases.mythos.firmansyah.co/channels/<channel>.json`.
2. When an update exists, the HMI status bar renders a subtle indicator badge: `[ 🚀 MythOS v1.2.0 Available - Click to Update ]`.
3. Selecting update initiates background download and SHA-256 verification.
4. The application prompts the operator to restart or schedules activation on the next studio launch.
