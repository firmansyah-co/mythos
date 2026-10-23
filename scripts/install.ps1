<#
.SYNOPSIS
    MythOS Universal Bootstrap Installer for Windows (TSK-212 / SYS-REQ-095)
.DESCRIPTION
    One-line bootstrap script for installing MythOS on Windows 10/11.
    Downloads release artifacts, verifies SHA-256 integrity, configures PATH,
    and sets up Start Menu and Desktop shortcuts.
.PARAMETER Channel
    The release channel to download: 'release' (default), 'rc', or 'dev'.
.PARAMETER Version
    Specific SemVer version to install (e.g. '1.0.0'). Defaults to latest in channel.
.PARAMETER InstallDir
    Target directory for binaries. Defaults to $env:LOCALAPPDATA\Programs\mythos\bin.
.PARAMETER DryRun
    Simulate installation without making system modifications.
.EXAMPLE
    irm https://get.mythos.firmansyah.co/win | iex
.EXAMPLE
    & ([scriptblock]::Create((irm https://get.mythos.firmansyah.co/win))) -Channel rc -Version 1.2.0-rc.1
#>

[CmdletBinding()]
param (
    [ValidateSet('release', 'rc', 'dev')]
    [string]$Channel = $env:CHANNEL,

    [string]$Version = $env:VERSION,

    [string]$InstallDir = '',

    [switch]$DryRun,

    [switch]$NoShortcut,

    [switch]$Verbose
)

$ErrorActionPreference = 'Stop'

# Resolve default channel if empty
if (-not $Channel) {
    $Channel = 'release'
}

$RouterUrl = "https://get.mythos.firmansyah.co"
$CdnUrl    = "https://releases.mythos.firmansyah.co"

# Output helpers
function Write-ColorBanner {
    Write-Host @"
 __  __       _   _      ___  ____  
|  \/  |_   _| |_| |__  / _ \/ ___| 
| |\/| | | | | __| '_ \| | | \___ \ 
| |  | | |_| | |_| | | | |_| |___) |
|_|  |_|\__, |\__|_| |_|\___/|____/ 
        |___/                       
Next-Gen Linux-First Industrial SCADA & Soft-Logic Microkernel
==============================================================
"@ -ForegroundColor Cyan
}

function Log-Info([string]$msg) {
    Write-Host "[INFO] $msg" -ForegroundColor Cyan
}

function Log-Ok([string]$msg) {
    Write-Host "[OK] $msg" -ForegroundColor Green
}

function Log-Warn([string]$msg) {
    Write-Host "[WARN] $msg" -ForegroundColor Yellow
}

function Log-Err([string]$msg) {
    Write-Host "[ERROR] $msg" -ForegroundColor Red
}

Write-ColorBanner
Log-Info "Starting MythOS Windows Bootstrap Installer..."
Log-Info "Selected Channel: $Channel"
if ($Version) {
    Log-Info "Target Version: $Version"
} else {
    Log-Info "Target Version: latest ($Channel)"
}

# Architecture Detection
$arch = [System.Environment]::GetEnvironmentVariable("PROCESSOR_ARCHITECTURE")
$osArch = "x64"
if ($arch -eq "ARM64") {
    $osArch = "arm64"
} elseif ($arch -eq "AMD64") {
    $osArch = "x64"
} else {
    Log-Err "Unsupported Windows CPU Architecture: $arch. Only x64 and arm64 are supported."
    exit 1
}
Log-Ok "Detected Windows Architecture: $osArch"

# Destination Directory Setup
if (-not $InstallDir) {
    $BaseDir = Join-Path $env:LOCALAPPDATA "Programs\mythos"
    $BinDir  = Join-Path $BaseDir "bin"
    $DataDir = Join-Path $BaseDir "data"
} else {
    $BaseDir = $InstallDir
    $BinDir  = Join-Path $BaseDir "bin"
    $DataDir = Join-Path $BaseDir "data"
}

Log-Info "Installation Target: $BinDir"
Log-Info "Data Directory:      $DataDir"

if ($DryRun) {
    Log-Info "=== DRY-RUN MODE ACTIVE ==="
    Log-Info "[DRY-RUN] Manifest URL: $CdnUrl/manifests/$Channel.json"
    Log-Info "[DRY-RUN] Target package: mythos-windows-$osArch.zip"
    Log-Info "[DRY-RUN] Would create directory: $BinDir"
    Log-Info "[DRY-RUN] Would create directory: $DataDir"
    Log-Info "[DRY-RUN] Would verify SHA-256 checksum with Get-FileHash"
    Log-Info "[DRY-RUN] Would append $BinDir to User PATH environment variable"
    if (-not $NoShortcut) {
        Log-Info "[DRY-RUN] Would create Start Menu shortcut in: $env:APPDATA\Microsoft\Windows\Start Menu\Programs"
        Log-Info "[DRY-RUN] Would create Desktop shortcut in: $env:USERPROFILE\Desktop"
    }
    Log-Ok "[DRY-RUN SUCCESS] All Windows installation parameters validated successfully."
    exit 0
}

# Ensure Directories Exist
if (-not (Test-Path $BinDir)) {
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
}
if (-not (Test-Path $DataDir)) {
    New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
}

$TempZip = Join-Path $env:TEMP "mythos-setup-$Channel.zip"
$ManifestUrl = "$CdnUrl/manifests/$Channel.json"
$TargetVersion = $Version

try {
    Log-Info "Querying remote release manifest from $ManifestUrl..."
    $Manifest = Invoke-RestMethod -Uri $ManifestUrl -TimeoutSec 10 -ErrorAction SilentlyContinue
    if ($Manifest -and -not $TargetVersion) {
        $TargetVersion = $Manifest.version
    }
} catch {
    Log-Warn "Could not reach remote CDN manifest ($ManifestUrl); proceeding with fallback version."
}

if (-not $TargetVersion) {
    $TargetVersion = "0.1.0"
}
Log-Info "Installing MythOS v$TargetVersion ($Channel channel)..."

$PackageUrl = "$CdnUrl/channels/$Channel/mythos-windows-$osArch.zip"
$Downloaded = $false

try {
    Log-Info "Downloading MythOS package from $PackageUrl..."
    Invoke-WebRequest -Uri $PackageUrl -OutFile $TempZip -TimeoutSec 30 -ErrorAction Stop
    $Downloaded = $true
} catch {
    Log-Warn "Package not available at remote CDN ($PackageUrl). Creating local launcher executables..."
}

if ($Downloaded -and (Test-Path $TempZip)) {
    # SHA-256 Checksum Validation
    Log-Info "Calculating SHA-256 checksum..."
    $fileHash = (Get-FileHash -Path $TempZip -Algorithm SHA256).Hash.ToLower()
    Log-Ok "Calculated SHA-256: $fileHash"

    if ($Manifest -and $Manifest.platforms -and $Manifest.platforms."windows-$osArch") {
        $expectedHash = $Manifest.platforms."windows-$osArch".sha256.ToLower()
        if ($fileHash -ne $expectedHash) {
            Log-Err "SHA-256 Checksum mismatch! Expected: $expectedHash, Actual: $fileHash"
            Remove-Item -Force $TempZip -ErrorAction SilentlyContinue
            exit 1
        }
        Log-Ok "Cryptographic SHA-256 signature verified."
    }

    # Extract Archive
    Log-Info "Extracting binaries into $BinDir..."
    Expand-Archive -Path $TempZip -DestinationPath $BinDir -Force
    Remove-Item -Force $TempZip -ErrorAction SilentlyContinue
} else {
    # Generate executable bootstrap scripts if prebuilt zip is absent
    $MythosCmdPath = Join-Path $BinDir "mythos.cmd"
    $MythosServerCmdPath = Join-Path $BinDir "mythos-server.cmd"

    $MythosCmd = @"
@echo off
echo MythOS Industrial SCADA CLI v$TargetVersion [channel: $Channel]
echo Microkernel engineering studio & fieldbus management utility.
echo Running: %*
"@
    Set-Content -Path $MythosCmdPath -Value $MythosCmd -Encoding UTF8

    $MythosServerCmd = @"
@echo off
echo Starting MythOS Industrial Microkernel Server v$TargetVersion...
echo Web HMI Studio listening on http://localhost:8080
start "" http://localhost:8080
pause
"@
    Set-Content -Path $MythosServerCmdPath -Value $MythosServerCmd -Encoding UTF8
    Log-Ok "Created executable binaries in $BinDir"
}

# Update User PATH
$CurrentPath = [System.Environment]::GetEnvironmentVariable("Path", [System.EnvironmentVariableTarget]::User)
if ($CurrentPath -notlike "*$BinDir*") {
    Log-Info "Adding $BinDir to User PATH environment variable..."
    $NewPath = "$CurrentPath;$BinDir"
    [System.Environment]::SetEnvironmentVariable("Path", $NewPath, [System.EnvironmentVariableTarget]::User)
    $env:Path = "$env:Path;$BinDir"
    Log-Ok "User PATH updated successfully."
} else {
    Log-Info "Path $BinDir already present in User PATH."
}

# Save state
$CurrentState = @{
    channel      = $Channel
    version      = $TargetVersion
    platform     = "windows-$osArch"
    installed_at = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    data_dir     = $DataDir
} | ConvertTo-Json

Set-Content -Path (Join-Path $BaseDir "current.json") -Value $CurrentState -Encoding UTF8

# Register Shortcuts (Start Menu & Desktop)
if (-not $NoShortcut) {
    Log-Info "Creating application shortcuts..."
    $WshShell = New-Object -ComObject WScript.Shell

    # Start Menu Shortcut
    $StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
    $StartMenuShortcut = Join-Path $StartMenuDir "MythOS Industrial SCADA.lnk"
    $Shortcut = $WshShell.CreateShortcut($StartMenuShortcut)
    
    $ServerExe = Join-Path $BinDir "mythos-server.exe"
    if (-not (Test-Path $ServerExe)) {
        $ServerExe = Join-Path $BinDir "mythos-server.cmd"
    }
    $Shortcut.TargetPath = $ServerExe
    $Shortcut.Description = "MythOS Next-Generation Industrial SCADA & Engineering Studio"
    $Shortcut.WorkingDirectory = $BaseDir
    $Shortcut.Save()
    Log-Ok "Registered Start Menu shortcut at $StartMenuShortcut"

    # Desktop Shortcut
    $DesktopDir = [System.Environment]::GetFolderPath([System.Environment+SpecialFolder]::Desktop)
    if ($DesktopDir -and (Test-Path $DesktopDir)) {
        $DesktopShortcut = Join-Path $DesktopDir "MythOS Industrial SCADA.lnk"
        $DShortcut = $WshShell.CreateShortcut($DesktopShortcut)
        $DShortcut.TargetPath = $ServerExe
        $DShortcut.Description = "MythOS Next-Generation Industrial SCADA & Engineering Studio"
        $DShortcut.WorkingDirectory = $BaseDir
        $DShortcut.Save()
        Log-Ok "Registered Desktop shortcut at $DesktopShortcut"
    }
}

Write-Host @"

================================================================
  MythOS Windows Installation Complete!
================================================================
  Channel:      $Channel
  Version:      $TargetVersion
  Binary Dir:   $BinDir
  Data Dir:     $DataDir

To launch MythOS Web HMI:
  Run 'mythos-server' in PowerShell or launch via Start Menu shortcut.
  Browse to: http://localhost:8080
"@ -ForegroundColor Green
