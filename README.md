# 🔐 Krypthor Vault (Rust & egui / Ratatui / Web Daemon)

![Krypthor Vault](resources/images/krypthor_banner.svg)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20macOS%20%7C%20Windows-brightgreen.svg)]()

**Krypthor Vault** (`krypthor`) is a modern, memory-safe, and high-performance zero-knowledge vault solution written in Pure Rust. It features a desktop UI built with `egui` and `egui-elegance`, a terminal interface (TUI via `ratatui`), a **Syncthing-like web dashboard and background daemon**, as well as a full command-line interface (CLI) with FUSE and WebDAV filesystem integrations.

## Features

- **Vault Creation**: Generates a cryptographic salt, derives an encryption key using `Scrypt`, and encrypts the master key via `AES-256-GCM`.
- **Vault Management**: Persistently stores registered vaults in `vaults.json`.
- **Desktop File Manager**: Manage, import, export, and delete encrypted files and directories directly via the graphical interface (`egui`).
- **Web Dashboard & Headless Daemon (Syncthing-Style)**:
  - Integrated pure-Rust web server via `axum` and `tokio`.
  - **Single-Binary**: Fully responsive HTML5/CSS3/JS single-page dashboard compiled directly into the binary (`src/web/index.html`).
  - Works in any browser on desktop, tablet, and smartphone (drag-and-drop upload, download, unlock, lock, text file creation).
  - **In-App Text Editor**: View and edit text and Markdown files directly within the web interface.
  - **Live File Search & Vault Statistics**: Instant full-text search across all folders, displaying file counts and storage usage.
  - **Password Change & Recovery Key**: Seamless in-place password updates, as well as export and unlock via a 64-character recovery key.
  - **Direct ZIP Export**: Download the entire decrypted vault or specific subfolders as a ZIP archive directly in the browser or via the REST API (`/api/vaults/export_zip?path=...&subfolder=...`).
  - **Configurable Auto-Lock**: Automatically locks and wipes decrypted secrets from memory upon inactivity (configurable: 5, 15, 30, 60 minutes or disabled) with a status badge.
  - REST API for automation (`/api/vaults`, `/api/vaults/rename_vault`, `/api/vaults/unlock`, `/api/vaults/recover`, `/api/vaults/files`, `/api/vaults/find`, `/api/vaults/stats`, `/api/vaults/export_zip`, `/api/vaults/change_password`, `/api/vaults/recovery_key`, `/api/vaults/backup_key`, `/api/vaults/restore_key`, `/api/vaults/copy`, `/api/vaults/hash`, `/api/vaults/prune`, `/api/system/benchmark`, `/api/system/doctor`, etc.).
- **Terminal User Interface (TUI)**: Full-featured TUI based on `ratatui` and `crossterm` for vault management and navigation directly in the terminal, including inactivity auto-lock.
- **FUSE Filesystem Integration**: Mount the encrypted vault directly as a virtual filesystem directory on Linux/Unix using pure-Rust FUSE (no C `libfuse` dependency).
- **Streaming I/O**: Direct stream processing (Reader/Writer) for `put`, `cat`, `import`, and `export` to minimize memory overhead when handling large files.
- **Transparent Zstd Compression & Versioned Payload Header**:
  - Every encrypted file includes a fixed 24-byte header (`b"KRYP"` magic bytes, version `v1`, compression flag).
  - Text and data files are transparently compressed with **`zstd`** prior to AES-256-GCM encryption by default.
  - Pre-compressed formats (`.jpg`, `.zip`, `.mp4`, `.pdf`, etc.) are automatically detected and stored without redundant compression overhead.
  - Fine-grained control per file and import (`--compression auto|always|never`).
  - Seamless backward compatibility fallback for unversioned legacy v0 payloads.
- **Disaster Recovery & Resilience**: Atomic writes using PID temporary files (`fs::rename`) and automated maintenance of `masterkey.json.bkup` with transparent unlock fallback.
- **CLI Commands**: Complete headless control via 22 subcommands (`create`, `mount`, `ls`, `find`, `put`, `cat`, `rm`, `cp`, `mv`, `stats`, `mkdir`, `export`, `import`, `verify`, `backup-key`, `restore-key`, `benchmark`, `doctor`, `hash`, `prune`, `tui`, `serve`).
- **Security Architecture**:
  - **Key Derivation**: Scrypt (N=16384, r=8, p=1) with optional support for keyfiles.
  - **File Encryption**: Authenticated Encryption with AES-256-GCM. Each file uses a unique 12-byte random nonce and encrypted path metadata.
  - **Filename Obfuscation**: Physical filenames on disk are deterministically obfuscated using `SHA-256(master_key || relative_path)`.
  - **Auto-Lock**: Automatic vault locking and memory wiping after a configurable duration of inactivity.
- **Code Quality & Modularity**:
  - Modular feature architecture: FUSE is decoupled as an optional cargo feature (`--features fuse`); default build is 100% portable and requires neither root/sudo privileges nor FUSE kernel drivers.
  - Strict zero-panic policy enforced via Clippy (`panic`, `unwrap_used`, `expect_used`, `panic_in_result_fn`, `unreachable`, `todo`, `unimplemented` set to `deny`).
  - License compliance audited via [`deny.toml`](deny.toml) and automated integration tests [`tests/test_license_compatibility.rs`](tests/test_license_compatibility.rs) ensuring all dependencies are MIT-compatible.
  - Cross-platform support (Linux X11/Wayland, macOS, Windows).

## Prerequisites & Installation

### Prerequisites (Linux)
Because the FUSE driver is implemented 100% in pure Rust, **no C development libraries (`libfuse3-dev`) are required to build**. For unprivileged desktop mounts and the GUI, only graphics libraries and the system utility `fuse3` (`fusermount3`) are needed:

```bash
# Debian / Ubuntu
sudo apt install fuse3 pkg-config libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev

# Fedora
sudo dnf install fuse3 pkgconf-pkg-config libX11-devel libXcursor-devel libXrandr-devel libXi-devel mesa-libGL-devel

# Arch Linux
sudo pacman -S fuse3 pkgconf libx11 libxcursor libxrandr libxi mesa
```

### 1-Click Installation (Syncthing-Style)

Krypthor Vault provides automated installation scripts for all three major platforms. These compile the release binary, set up desktop launchers/icons, and configure a **Syncthing-style background daemon** (autostart on login, web dashboard on port 8080):

#### Linux (`systemd --user` + Desktop Launcher)
```bash
./scripts/install-linux.sh

# For clean uninstallation:
./scripts/install-linux.sh --uninstall
```

#### macOS (`launchd` LaunchAgent + Web App)
```bash
./scripts/install-macos.sh

# For clean uninstallation:
./scripts/install-macos.sh --uninstall
```

#### Windows (Scheduled Task + Start Menu / Desktop Shortcuts)
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install-windows.ps1

# For clean uninstallation:
powershell -ExecutionPolicy Bypass -File .\scripts\install-windows.ps1 -Uninstall
```

### Manual Build

```bash
# Default build (portable, without FUSE dependency)
cargo build --release

# With FUSE filesystem support (Linux/Unix)
cargo build --release --features fuse
```

## Usage

### 1. Graphical User Interface (GUI)
By default, `krypthor` launches in desktop GUI mode:
```bash
cargo run --release
```

### 2. Terminal User Interface (TUI)
For intuitive management in the terminal:
```bash
cargo run --release -- tui
# Alternatively:
cargo run --release -- --tui
```
- `[↑/↓]` or `[j/k]`: Select vault
- `[Enter]` or `[u]`: Unlock vault with password & optional keyfile, activate FUSE mount
- `[v]`: Run cryptographic integrity check on unlocked vault
- `[b]`: Create master key backup for selected vault
- `[n]`: Rename selected vault
- `[d]`: Remove selected vault from registry
- `[r]`: Recover & mount vault using 64-character recovery key
- `[x]`: Display recovery key of unlocked vault
- `[l]`: Lock & unmount vault
- `[a]`: Register existing vault path
- `[c]`: Interactively create a new vault (4-step wizard including optional 2FA keyfile)
- `[q]`: Quit TUI

### 3. Web Client & Daemon (Syncthing-Style)
Starts the background daemon with an embedded web dashboard:
```bash
cargo run --release -- serve --host 127.0.0.1 --port 8080
# Alternatively:
cargo run --release -- --serve
```
Then open **`http://127.0.0.1:8080`** in your browser to manage, unlock, and transfer files via drag-and-drop.

### 4. CLI Commands
The application can also be controlled directly via the command line. All commands that create, unlock, or modify a vault support the optional `--keyfile <PATH>` flag for two-factor protection:

#### List vaults
```bash
cargo run --release -- list
```

#### Register existing vault (add)
```bash
cargo run --release -- add --path "/path/to/vault" --name "MyVault"
```

#### Remove vault from registry (remove)
```bash
cargo run --release -- remove --path "/path/to/vault"
```

#### Rename registered vault (rename-vault)
```bash
cargo run --release -- rename-vault --path "/path/to/vault" --new-name "NewVaultName"
```

#### Create new vault
```bash
cargo run --release -- create --name "MyVault" --path "/path/to/vault"
# Optionally provide a password (an interactive prompt will appear if omitted):
cargo run --release -- create --name "MyVault" --path "/path/to/vault" --password "secret"
```

#### Serve vault as WebDAV network share (Standard build, cross-platform)
Starts a local WebDAV endpoint for native operating system mounting without kernel drivers or administrator/root privileges:
```bash
cargo run --release -- webdav --path "/path/to/vault"
# With custom port and password:
cargo run --release -- webdav --path "/path/to/vault" --port 8081 --password "secret"
```

**Operating System Connection (WebDAV):**
* **Windows (Explorer):** *Map network drive* ➔ Address: `http://localhost:8081/`
* **macOS (Finder):** *Go* ➔ *Connect to Server* (`⌘K`) ➔ `http://localhost:8081/`
* **Linux (GNOME Files / Dolphin):** *Connect to Server* ➔ `dav://localhost:8081/`

*Note: When the web daemon (`cargo run --release -- serve`) is running, all vaults unlocked in the web dashboard are automatically accessible via `http://localhost:8080/webdav/<VaultName>/` in your file manager.*

#### Mount vault (FUSE, requires `--features fuse`)
```bash
cargo run --release --features fuse -- mount --path "/path/to/vault" --mountpoint "/path/to/mountpoint"
# Optionally with password:
cargo run --release --features fuse -- mount --path "/path/to/vault" --mountpoint "/path/to/mountpoint" --password "secret"
```

To unmount the FUSE directory:
```bash
cargo run --release --features fuse -- unmount --mountpoint "/path/to/mountpoint"
# Or via fusermount:
fusermount -u "/path/to/mountpoint"
```

#### Change password
```bash
cargo run --release -- change-password --path "/path/to/vault"
# Or with parameters:
cargo run --release -- change-password --path "/path/to/vault" --old-password "old_pw" --new-password "new_pw"
```

#### Export recovery key
```bash
cargo run --release -- recovery-key --path "/path/to/vault"
```

#### Verify / unlock vault with recovery key
```bash
cargo run --release -- recover --path "/path/to/vault" --key "XXXX-XXXX-XXXX-..."
```

#### List files in vault (ls)
```bash
cargo run --release -- ls --path "/path/to/vault"
# With file size and modification date (-l / --long):
cargo run --release -- ls --path "/path/to/vault" -l
# With pattern filter:
cargo run --release -- ls --path "/path/to/vault" --pattern "documents"
```

#### Search files (find)
Search for files matching a query string (optionally with detailed view `-l`):
```bash
cargo run --release -- find --path "/path/to/vault" -q "report"
# With detailed view:
cargo run --release -- find --path "/path/to/vault" -q "report" -l
```

#### Encrypt & store file in vault (put)
By default, intelligent `auto` compression with Zstd is applied (compresses text/JSON/code, skips incompressible formats like JPG, ZIP, MP4):
```bash
cargo run --release -- put --path "/path/to/vault" --source "local_file.txt" --target "folder/file.txt"

# Explicitly control compression (--compression auto|always|never):
cargo run --release -- put --path "/path/to/vault" --source "backup.raw" --target "folder/backup.raw" --compression always
cargo run --release -- put --path "/path/to/vault" --source "data.bin" --target "folder/data.bin" --compression never
```

#### Decrypt & display file content (cat)
```bash
cargo run --release -- cat --path "/path/to/vault" --file "folder/file.txt"
```

#### Delete file or folder in vault (rm)
By default, deleted files and folders are moved to the vault-internal trash bin (`.trash/`) and can be restored. Use `--permanent` to immediately delete them irrevocably from disk:
```bash
# Move to trash bin:
cargo run --release -- rm --path "/path/to/vault" --file "folder/file.txt"

# Move directory to trash bin recursively (-r / --recursive):
cargo run --release -- rm --path "/path/to/vault" --file "folder" -r

# Delete permanently without trash bin (--permanent):
cargo run --release -- rm --path "/path/to/vault" --file "folder/file.txt" --permanent
```

#### Manage trash bin (trash)
Manage the vault-internal trash bin for accidentally deleted files:
```bash
# List all files in the trash bin (optionally with --json):
cargo run --release -- trash list --path "/path/to/vault"
cargo run --release -- trash list --path "/path/to/vault" --json

# Restore a file or directory from the trash bin:
cargo run --release -- trash restore --path "/path/to/vault" -t ".trash/1729780000##folder/file.txt"

# Empty the trash bin completely and irrevocably:
cargo run --release -- trash empty --path "/path/to/vault"

# Prune files in the trash bin older than N days (retention policy):
cargo run --release -- trash prune --path "/path/to/vault" --days 30
cargo run --release -- trash prune --path "/path/to/vault" --days 30 --json
```

#### JSON output for scripting & CI/CD (--json)
All read-only inspection commands (`list`, `ls`, `find`, `stats`, `verify`, `recovery-key`) support the `--json` flag for machine-readable JSON output:
```bash
cargo run --release -- ls --path "/path/to/vault" --json
cargo run --release -- stats --path "/path/to/vault" --json
cargo run --release -- verify --path "/path/to/vault" --json
```

#### Move / rename file or folder in vault (mv)
```bash
cargo run --release -- mv --path "/path/to/vault" --old-path "documents/old.txt" --new-path "archive/new.txt"
```

#### Copy file or folder in vault (cp)
Copies files or entire directory trees recursively inside the vault without decrypting them to the host filesystem:
```bash
# Duplicate single file
cargo run --release -- cp --path "/path/to/vault" --src "documents/invoice.pdf" --dst "documents/invoice_copy.pdf"

# Recursively copy entire directory (-r)
cargo run --release -- cp --path "/path/to/vault" --src "projects/alpha" --dst "archive/alpha_backup" -r
```

#### Retrieve vault statistics (stats)
```bash
cargo run --release -- stats --path "/path/to/vault"
```

#### Create directory in vault (mkdir)
```bash
cargo run --release -- mkdir --path "/path/to/vault" --folder "projects/secret"
```

#### Export vault or subfolder to local directory (export)
```bash
# Export entire vault
cargo run --release -- export --path "/path/to/vault" --dest "/path/to/destination"

# Export specific subfolder only
cargo run --release -- export --path "/path/to/vault" --dest "/path/to/destination" --subfolder "documents/projects"
```

#### Recursively import local directory into vault (import)
```bash
cargo run --release -- import --path "/path/to/vault" --source "/path/to/source" --dest "backup"

# Optionally specify compression mode (--compression auto|always|never):
cargo run --release -- import --path "/path/to/vault" --source "/path/to/source" --dest "backup" --compression auto
```

#### Cryptographic vault integrity verification (verify)
Verifies all encrypted files in the vault against their AES-256-GCM authentication tags to detect bitrot, tampering, or orphaned files:
```bash
cargo run --release -- verify --path "/path/to/vault"
```

#### Backup master key (backup-key)
Creates an explicit, timestamped or custom backup copy of `masterkey.json`:
```bash
cargo run --release -- backup-key --path "/path/to/vault"
# Or with a custom destination path:
cargo run --release -- backup-key --path "/path/to/vault" --out "/backup/vault_key.json"
```

#### Restore master key (restore-key)
Atomically restores `masterkey.json` from the automated `masterkey.json.bkup` or a specified backup file:
```bash
cargo run --release -- restore-key --path "/path/to/vault"
# Or from a specific backup file:
cargo run --release -- restore-key --path "/path/to/vault" --from "/backup/vault_key.json"
```

#### Run cryptographic performance benchmark (benchmark)
Measures the real-world throughput of the cryptographic primitives (Scrypt KDF, AES-256-GCM encryption/decryption in MB/s, and SHA-256 path obfuscation):
```bash
cargo run --release -- benchmark
# Machine-readable JSON output:
cargo run --release -- benchmark --json
```

#### Check system environment and vault health (doctor)
Inspects the filesystem, FUSE permissions (`/dev/fuse`), CSPRNG random generators, and the integrity of all registered vaults:
```bash
cargo run --release -- doctor
# Machine-readable JSON output:
cargo run --release -- doctor --json
```

#### Calculate SHA-256 checksum of a file in vault (hash)
Streams and calculates the SHA-256 hash directly from the encrypted vault without decrypting the file onto the host disk:
```bash
cargo run --release -- hash --path "/path/to/vault" --file "documents/invoice.pdf"
# Machine-readable JSON output:
cargo run --release -- hash --path "/path/to/vault" --file "documents/invoice.pdf" --json
```

#### Prune orphaned and temporary fragments (prune)
Cleans up incomplete upload fragments (`.tmp`) and orphaned files from the storage directory `d/`:
```bash
# Preview without deleting (--dry-run)
cargo run --release -- prune --path "/path/to/vault" --dry-run

# Perform actual pruning
cargo run --release -- prune --path "/path/to/vault"
```

## Project Structure

- [`src/lib.rs`](src/lib.rs): Library crate root for module and integration linking.
- [`src/main.rs`](src/main.rs): Entry point, CLI argument parsing (`clap`), and command dispatching.
- [`src/vault.rs`](src/vault.rs): Cryptographic core (Scrypt KDF, AES-256-GCM, path hashing, vault initialization & file encryption).
- [`src/webdav_fs.rs`](src/webdav_fs.rs): Cross-platform WebDAV filesystem implementation (`dav-server`) for native network drive integration.
- [`src/web_server.rs`](src/web_server.rs): Web server and REST API via `axum` and `tokio`.
- [`src/web/index.html`](src/web/index.html): Embedded responsive web dashboard (single-page app).
- [`src/egui_ui.rs`](src/egui_ui.rs): Desktop GUI based on `eframe`, `egui`, and `egui-elegance`.
- [`src/fs_fuse.rs`](src/fs_fuse.rs): Virtual FUSE filesystem (Unix/Linux, optional via `--features fuse`).
- [`src/tui_app.rs`](src/tui_app.rs): Terminal user interface based on `ratatui` and `crossterm`.
- [`tests/test_webdav.rs`](tests/test_webdav.rs): Integration and routing tests for the WebDAV server.
- [`tests/test_license_compatibility.rs`](tests/test_license_compatibility.rs): Automated test verifying MIT license compatibility across the crate dependency tree.
- [`deny.toml`](deny.toml): Configuration for `cargo-deny`.

## Tests & Code Quality

```bash
# Run tests
cargo test

# Clippy linter
cargo clippy

# Check formatting
cargo fmt -- --check
```

## License

This project is licensed under the MIT License.
