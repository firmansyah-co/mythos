# MythOS Architecture Specification: One-Click Desktop Installers & Universal Distribution

- **Document Identifier**: MYTHOS-ARCH-008
- **Version**: 1.1.0
- **Author**: Neo (VP Software & Infrastructure)
- **Status**: APPROVED / SPECIFICATION
- **Language**: ASD-STE100 Technical English

---

## 1. Scope and Objective

This specification establishes the official **One-Click Distribution Strategy** for MythOS.

In response to direct mandates from Adrian (Board of Commissioners), installing MythOS must be as seamless and familiar as installing Siemens WinCC or modern desktop developer applications (e.g., VSCode, Docker Desktop, Obsidian). Users on **Windows**, **macOS**, and **Linux** must not be forced to install compilers, configure shell scripts, or compile code manually from source to launch the engineering studio.

---

## 2. Universal Distribution Artifacts by Operating System

MythOS automated CI/CD pipelines compile and package official production installers for every platform on each release:

```
+----------------------------------------------------------------------------------------------------+
|                                MYTHOS UNIVERSAL INSTALLER SUITE                                    |
|                                                                                                    |
|  [ WINDOWS 10 / 11 ]        [ macOS (Apple/Intel) ]     [ LINUX DESKTOPS ]      [ HEADLESS CONTROLLER ]|
|  MythOS-Setup-x64.exe       MythOS-Universal.dmg        MythOS-x86_64.AppImage  curl | bash Installer  |
|  • Single-click wizard      • Drag to /Applications     • Double-click & run    • 1-command auto-setup |
|  • Bundled runtime service  • Native Cocoa WebView      • Portable zero-install • systemd service reg  |
|  • Start Menu integration   • Code-signed & notarized   • .deb / .rpm packages  • RPi / IPC headless   |
+----------------------------------------------------------------------------------------------------+
```

---

## 3. Platform Packaging Specifications

### 3.1 Windows 10 & 11: Single-Click Setup (`MythOS-Setup-x64.exe`)
- **Packaging Engine**: NSIS (Nullsoft Scriptable Install System) / WiX Toolset via Tauri v2.
- **User Experience**:
  * Double-click `MythOS-Setup-x64.exe`.
  * Standard graphical setup wizard: License Agreement ➔ Install Location (`C:\Program Files\MythOS`) ➔ Desktop & Start Menu Shortcuts.
  * Optional checkbox: *"Install MythOS Background Controller Service (Auto-Start with Windows)"*.
- **Bundled Payloads**:
  * Standalone `mythos-server.exe` microkernel binary.
  * Bundled web studio UI assets.
  * Bundled Clang/LLVM C++ compiler toolchain (zero compiler installation required from user).
- **Silent Installation Support (Enterprise Fleet Deployment)**:
  ```powershell
  MythOS-Setup-x64.exe /S /D=C:\Program Files\MythOS
  ```

### 3.2 macOS (Apple Silicon & Intel): Universal Disk Image (`MythOS-Universal.dmg`)
- **Packaging Engine**: macOS `.dmg` bundle with native Apple Silicon (arm64) and Intel (x86_64) Universal 2 binary.
- **User Experience**:
  * Double-click `MythOS-Universal.dmg`.
  * Standard macOS installation window: Drag the **MythOS icon into the `/Applications` folder**.
  * Launch from Launchpad or Spotlight.
  * Apple Notarization: Signed with Apple Developer ID certificate, eliminating *"Unidentified Developer"* security prompts.
- **WebView Engine**: Uses native macOS `WKWebView` (zero Chromium memory bloat).

### 3.3 Linux (All Distributions): Universal AppImage & Packages
- **Primary Package: Universal `MythOS-x86_64.AppImage`**:
  * Single executable file containing all shared libraries and dependencies.
  * Runs out of the box on **Ubuntu, Debian, Fedora, Arch Linux, Linux Mint, openSUSE, and CentOS**.
  * User simply makes it executable and runs:
    ```bash
    chmod +x MythOS-x86_64.AppImage
    ./MythOS-x86_64.AppImage
    ```
- **Native Distro Packages**:
  * Debian / Ubuntu: `mythos_amd64.deb` (`sudo dpkg -i mythos_amd64.deb`).
  * Fedora / RHEL: `mythos.x86_64.rpm` (`sudo rpm -i mythos.rpm`).
  * Arch Linux: `mythos-bin` on the Arch User Repository (AUR).

### 3.4 Headless Industrial IPC & Raspberry Pi: One-Line Curl Installer
For headless field controllers (Raspberry Pi 4/5, CM4, Advantech, Siemens IPC) without desktop monitors:
- **Command**:
  ```bash
  curl -fsSL https://get.mythos.firmansyah.co | sudo bash
  ```
- **Automated Workflow**:
  1. Detects CPU architecture (`x86_64` vs `aarch64` / `armv7l`).
  2. Downloads the matching official release binary from GitHub Releases CDN.
  3. Verifies cryptographic SHA-256 checksum against central registry.
  4. Configures system user `mythos` with `uucp` / `dialout` serial permissions and `CAP_NET_RAW` / `CAP_SYS_NICE`.
  5. Registers and enables systemd service `mythos.service` (`systemctl enable --now mythos`).
  6. Outputs ready status: *"MythOS is live at http://<ip_address>:8080"*.

---

## 4. In-App Auto-Update Architecture

The desktop application integrates an automated update checker (Tauri Updater):
1. On application launch, MythOS checks `https://releases.mythos.firmansyah.co/update.json`.
2. When a newer version exists, a subtle non-intrusive notification badge appears in the bottom status bar: `[ 🚀 MythOS v1.2.0 Available - Click to Update ]`.
3. Clicking update performs background delta downloading, SHA-256 verification, and applies the update upon application restart without user friction.
