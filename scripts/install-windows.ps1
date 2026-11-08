<#
.SYNOPSIS
    Krypthor Vault - Windows Installation Script (Syncthing-Style)

.DESCRIPTION
    - Builds optimized release binary using cargo
    - Installs binary and assets into %LOCALAPPDATA%\Programs\Krypthor
    - Appends directory to user PATH environment variable
    - Creates Start Menu and Desktop shortcuts (GUI & Web Dashboard)
    - Sets up background auto-start on user logon (Syncthing-style silent daemon)
    - Supports clean uninstall (-Uninstall)

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File .\scripts\install-windows.ps1
    powershell -ExecutionPolicy Bypass -File .\scripts\install-windows.ps1 -Uninstall
#>

[CmdletBinding()]
param (
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"

$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Krypthor"
$TargetBin = Join-Path $InstallDir "krypthor.exe"
$TaskName = "KrypthorVaultDaemon"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$StartMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Krypthor"
$DesktopDir = [Environment]::GetFolderPath("Desktop")

function Write-Info($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Write-Success($msg) { Write-Host "==> $msg" -ForegroundColor Green }
function Write-Warn($msg) { Write-Host "==> $msg" -ForegroundColor Yellow }
function Write-Err($msg) { Write-Host "==> $msg" -ForegroundColor Red }

# ------------------------------------------------------------------------------
# Uninstall Mode
# ------------------------------------------------------------------------------
if ($Uninstall) {
    Write-Info "Uninstalling Krypthor Vault (Windows)..."

    # Stop running processes
    Get-Process -Name "krypthor" -ErrorAction SilentlyContinue | Stop-Process -Force

    # Remove Scheduled Task
    if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
        Write-Info "Removing background scheduled task '$TaskName'..."
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    }

    # Remove Startup shortcut if present
    $StartupDir = [Environment]::GetFolderPath("Startup")
    $StartupLnk = Join-Path $StartupDir "Krypthor Vault Daemon.lnk"
    if (Test-Path $StartupLnk) { Remove-Item -Path $StartupLnk -Force }

    # Remove shortcuts
    if (Test-Path $StartMenuDir) { Remove-Item -Path $StartMenuDir -Recurse -Force }
    $DesktopLnk1 = Join-Path $DesktopDir "Krypthor Vault.lnk"
    $DesktopLnk2 = Join-Path $DesktopDir "Krypthor Web Dashboard.lnk"
    if (Test-Path $DesktopLnk1) { Remove-Item -Path $DesktopLnk1 -Force }
    if (Test-Path $DesktopLnk2) { Remove-Item -Path $DesktopLnk2 -Force }

    # Remove installation files
    if (Test-Path $InstallDir) {
        Write-Info "Removing files from '$InstallDir'..."
        Remove-Item -Path $InstallDir -Recurse -Force
    }

    # Clean PATH
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -like "*$InstallDir*") {
        $NewPath = ($UserPath -split ';' | Where-Object { $_ -ne $InstallDir -and $_ -ne "" }) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
        Write-Info "Removed '$InstallDir' from User PATH."
    }

    Write-Success "Krypthor Vault uninstalled successfully."
    exit 0
}

# ------------------------------------------------------------------------------
# Prerequisites Check
# ------------------------------------------------------------------------------
Write-Info "Checking prerequisites..."

if (-not (Get-Command "cargo" -ErrorAction SilentlyContinue)) {
    Write-Err "Rust & Cargo are required to compile Krypthor. Install via https://rustup.rs"
    exit 1
}

# ------------------------------------------------------------------------------
# Build Release Binary
# ------------------------------------------------------------------------------
Write-Info "Building Krypthor Vault in release mode..."
Push-Location $RepoRoot
try {
    & cargo build --release
} finally {
    Pop-Location
}

$BuiltBin = Join-Path $RepoRoot "target\release\krypthor.exe"
if (-not (Test-Path $BuiltBin)) {
    Write-Err "Build failed: '$BuiltBin' not found."
    exit 1
}

# ------------------------------------------------------------------------------
# Install Files
# ------------------------------------------------------------------------------
Write-Info "Installing files to '$InstallDir'..."
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null

# Stop previous process if running
Get-Process -Name "krypthor" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

Copy-Item -Path $BuiltBin -Destination $TargetBin -Force

# Copy Icon
$IconSrc = Join-Path $RepoRoot "resources\images\krypthor.png"
if (Test-Path $IconSrc) {
    Copy-Item -Path $IconSrc -Destination (Join-Path $InstallDir "krypthor.png") -Force
}

# Add to User PATH
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    $NewPath = "$UserPath;$InstallDir"
    [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
    $env:Path = "$env:Path;$InstallDir"
    Write-Info "Appended '$InstallDir' to User PATH."
}

# ------------------------------------------------------------------------------
# Create Start Menu & Desktop Shortcuts
# ------------------------------------------------------------------------------
Write-Info "Creating shortcuts..."
New-Item -ItemType Directory -Path $StartMenuDir -Force | Out-Null

$WshShell = New-Object -ComObject WScript.Shell

# 1. Desktop GUI shortcut
$GuiShortcut = $WshShell.CreateShortcut((Join-Path $StartMenuDir "Krypthor Vault.lnk"))
$GuiShortcut.TargetPath = $TargetBin
$GuiShortcut.Description = "Krypthor Vault - Zero-Knowledge Encrypted Vaults"
$GuiShortcut.WorkingDirectory = $InstallDir
$GuiShortcut.Save()

# Copy to Desktop
Copy-Item (Join-Path $StartMenuDir "Krypthor Vault.lnk") (Join-Path $DesktopDir "Krypthor Vault.lnk") -Force

# 2. Web Dashboard shortcut
$WebShortcut = $WshShell.CreateShortcut((Join-Path $StartMenuDir "Krypthor Web Dashboard.lnk"))
$WebShortcut.TargetPath = "http://127.0.0.1:8080"
$WebShortcut.Description = "Open Krypthor Vault Web Dashboard"
$WebShortcut.Save()

Copy-Item (Join-Path $StartMenuDir "Krypthor Web Dashboard.lnk") (Join-Path $DesktopDir "Krypthor Web Dashboard.lnk") -Force

# ------------------------------------------------------------------------------
# Syncthing-Style Background Daemon (Scheduled Task / Logon)
# ------------------------------------------------------------------------------
Write-Info "Configuring Syncthing-style background startup on user logon..."

# Unregister previous task if exists
if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}

$Action = New-ScheduledTaskAction -Execute $TargetBin -Argument "serve --host 127.0.0.1 --port 8080" -WorkingDirectory $InstallDir
$Trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$Settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit 0 -Priority 7

Register-ScheduledTask -TaskName $TaskName -Action $Action -Trigger $Trigger -Settings $Settings -Description "Krypthor Vault Syncthing-style Background Web Daemon" | Out-Null

# Start service immediately
Start-ScheduledTask -TaskName $TaskName
Write-Success "Background daemon started."

# ------------------------------------------------------------------------------
# Summary
# ------------------------------------------------------------------------------
Write-Host ""
Write-Host "===================================================================" -ForegroundColor Green
Write-Host "       Krypthor Vault installed successfully (Syncthing-Style)     " -ForegroundColor Green
Write-Host "===================================================================" -ForegroundColor Green
Write-Host ""
Write-Host "  • Web Dashboard:   http://127.0.0.1:8080" -ForegroundColor Cyan
Write-Host "  • WebDAV Mount:    http://127.0.0.1:8080/webdav/<VaultName>/" -ForegroundColor Cyan
Write-Host "  • Binary:          $TargetBin" -ForegroundColor Cyan
Write-Host "  • Start Menu:      Krypthor Vault & Krypthor Web Dashboard" -ForegroundColor Cyan
Write-Host "  • Scheduled Task:  $TaskName (runs automatically in background on login)" -ForegroundColor Cyan
Write-Host ""
Write-Host "Native Netzlaufwerk in Windows Explorer:" -ForegroundColor Yellow
Write-Host "  Rechtsklick auf 'Dieser PC' -> 'Netzlaufwerk verbinden' -> Adresse:"
Write-Host "  http://localhost:8080/webdav/<VaultName>/" -ForegroundColor Cyan
Write-Host ""
Write-Host "To uninstall: powershell -ExecutionPolicy Bypass -File .\scripts\install-windows.ps1 -Uninstall"
Write-Host ""
