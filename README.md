# Cryptomator RS (Rust & egui / Ratatui)

Eine moderne, speichersichere und performante Implementierung des Cryptomator-Konzepts in Rust mit einer Desktop-Oberfläche auf Basis von `egui` und `egui-elegance`, einem Terminal-Interface (TUI via `ratatui`) sowie einer vollständigen Befehlszeilenschnittstelle (CLI) inklusive FUSE-Dateisystemintegration.

## Features

- **Erstellung neuer Tresore**: Generiert ein kryptografisches Salz, leitet einen Schlüssel mittels `Scrypt` ab und verschlüsselt den Masterkey via `AES-256-GCM`.
- **Tresor-Verwaltung**: Speichert registrierte Tresore persistent in `vaults.json`.
- **Desktop-Dateimanager**: Ermöglicht das Anlegen, Importieren, Exportieren und Löschen von verschlüsselten Dateien und Verzeichnissen direkt über die grafische Oberfläche (`egui`).
- **Terminal User Interface (TUI)**: Volles TUI-Interface basierend auf `ratatui` und `crossterm` zur Verwaltung und Navigation im Terminal.
- **FUSE Dateisystem-Integration**: Erlaubt unter Linux/Unix das Einbinden des verschlüsselten Tresors direkt als virtuelles Verzeichnis im Dateisystem via `fuser` (FUSE 3).
- **CLI-Befehle**: Vollwertige Steuerung ohne GUI über Subcommands (`list`, `create`, `mount`, `tui`).
- **Sicherheitsarchitektur**:
  - **Schlüsselableitung**: Scrypt (N=16384, r=8, p=1) mit optionaler Unterstützung für Schlüsseldateien (Keyfiles).
  - **Dateiverschlüsselung**: Authenticated Encryption mit AES-256-GCM. Jede Datei enthält einen zufälligen 12-Byte-Nonce und verschlüsselte Pfadmetadaten.
  - **Dateinamen-Verschleierung**: Physikalische Dateinamen auf dem Datenträger werden deterministisch durch `SHA-256(master_key || relativer_pfad)` verschleiert.
  - **Auto-Lock**: Automatische Sperrung des entsperrten Tresors bei Inaktivität nach konfigurierbarer Zeit.
- **Code-Qualität & Compliance**:
  - Strikte Zero-Panic-Policy via Clippy (`panic`, `unwrap_used`, `expect_used`, `panic_in_result_fn`, `unreachable`, `todo`, `unimplemented` auf `deny`).
  - Lizenzprüfung via [`deny.toml`](deny.toml) und automatisierter Integrationstest [`tests/test_license_compatibility.rs`](tests/test_license_compatibility.rs) zur Sicherstellung der MIT-Kompatibilität aller Abhängigkeiten.
  - Plattformübergreifend lauffähig (Linux X11/Wayland, macOS, Windows).

## Voraussetzungen & Installation

### Voraussetzungen (Linux)
Für die grafische Oberfläche und das FUSE-Mounting werden unter Linux FUSE 3- sowie Grafik-Bibliotheken benötigt:

```bash
# Debian / Ubuntu
sudo apt install libfuse3-dev pkg-config libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev

# Fedora
sudo dnf install fuse3-devel pkgconf-pkg-config libX11-devel libXcursor-devel libXrandr-devel libXi-devel mesa-libGL-devel

# Arch Linux
sudo pacman -S fuse3 pkgconf libx11 libxcursor libxrandr libxi mesa
```

### Bauen
```bash
cargo build --release
```

## Verwendung

### 1. Grafische Oberfläche (GUI)
Standardmäßig startet `cryptomator_rs` im Desktop-GUI-Modus:
```bash
cargo run --release
```

### 2. Terminal User Interface (TUI)
Für die Bedienung im Terminal:
```bash
cargo run --release -- tui
# Alternativ:
cargo run --release -- --tui
```

### 3. CLI-Befehle
Die Anwendung kann auch direkt per Befehlszeile gesteuert werden:

#### Tresore auflisten
```bash
cargo run --release -- list
```

#### Neuen Tresor anlegen
```bash
cargo run --release -- create --name "MeinTresor" --path "/pfad/zum/tresor"
# Optional mit Passwort (wenn weggelassen, erfolgt eine interaktive Eingabeaufforderung):
cargo run --release -- create --name "MeinTresor" --path "/pfad/zum/tresor" --password "geheim"
```

#### Tresor mounten (FUSE)
```bash
cargo run --release -- mount --path "/pfad/zum/tresor" --mountpoint "/pfad/zum/mountpunkt"
# Optional mit Passwort:
cargo run --release -- mount --path "/pfad/zum/tresor" --mountpoint "/pfad/zum/mountpunkt" --password "geheim"
```

Zum Aushängen des FUSE-Verzeichnisses:
```bash
fusermount -u "/pfad/zum/mountpunkt"
```

## Projektstruktur

- [`src/main.rs`](src/main.rs): Einstiegspunkt, CLI-Argument-Parsing (`clap`) und Befehls-Dispatching.
- [`src/vault.rs`](src/vault.rs): Kryptografischer Kern (Scrypt KDF, AES-256-GCM, Pfad-Hashing, Tresor-Initialisierung & Dateiverschlüsselung).
- [`src/egui_ui.rs`](src/egui_ui.rs): Desktop-GUI basierend auf `eframe`, `egui` und `egui-elegance`.
- [`src/fs_fuse.rs`](src/fs_fuse.rs): Virtuelles FUSE-Dateisystem (Unix/Linux) zur nativen Einbindung von Tresoren.
- [`src/tui_app.rs`](src/tui_app.rs): Terminal-Benutzeroberfläche basierend auf `ratatui` und `crossterm`.
- [`tests/test_license_compatibility.rs`](tests/test_license_compatibility.rs): Automatisierter Test zur Verifikation der MIT-Lizenzkompatibilität des Crate-Baums.
- [`deny.toml`](deny.toml): Konfiguration für `cargo-deny`.

## Tests & Code-Qualität

```bash
# Tests ausführen
cargo test

# Clippy Linter
cargo clippy

# Formatierung prüfen
cargo fmt -- --check
```

## Lizenz

Dieses Projekt ist unter der MIT-Lizenz lizenziert.
