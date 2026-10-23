# Cryptomator RS (Rust & egui / Ratatui / Web Daemon)

Eine moderne, speichersichere und performante Implementierung des Cryptomator-Konzepts in Rust mit einer Desktop-Oberfläche auf Basis von `egui` und `egui-elegance`, einem Terminal-Interface (TUI via `ratatui`), einem **Syncthing-ähnlichen Web-Dashboard / Hintergrund-Daemon** sowie einer vollständigen Befehlszeilenschnittstelle (CLI) inklusive FUSE-Dateisystemintegration.

## Features

- **Erstellung neuer Tresore**: Generiert ein kryptografisches Salz, leitet einen Schlüssel mittels `Scrypt` ab und verschlüsselt den Masterkey via `AES-256-GCM`.
- **Tresor-Verwaltung**: Speichert registrierte Tresore persistent in `vaults.json`.
- **Desktop-Dateimanager**: Ermöglicht das Anlegen, Importieren, Exportieren und Löschen von verschlüsselten Dateien und Verzeichnissen direkt über die grafische Oberfläche (`egui`).
- **Web-Dashboard & Headless Daemon (Syncthing-Stil)**:
  - Integrierter Pure-Rust Webserver via `axum` und `tokio`.
  - **Single-Binary**: Vollwertiges, responsives HTML5/CSS3/JS Single-Page-Dashboard direkt in die Binary einkompiliert (`src/web/index.html`).
  - Funktioniert im Browser auf Desktop, Tablet und Smartphone (Drag & Drop Upload, Download, Entsperren, Sperren, Textdateien erstellen).
  - **In-App Text-Editor**: Text- und Markdown-Dateien können direkt im Web-Interface betrachtet und bearbeitet werden.
  - **Live-Dateisuche & Tresorstatistiken**: Sofortige Volltextsuche über alle Ordner hinweg sowie Anzeige von Dateianzahl und belegtem Speicherplatz.
  - **Passwortänderung & Wiederherstellungsschlüssel**: Bequeme Passwortaktualisierung im laufenden Betrieb sowie Export und Entsperrung via 64-stelligem Recovery-Key.
  - **Auto-Lock**: Automatische Sperrung und sichere Speicherbereinigung entsperrter Tresore nach 30 Minuten Inaktivität.
  - REST-API für Automatisierung (`/api/vaults`, `/api/vaults/rename_vault`, `/api/vaults/unlock`, `/api/vaults/recover`, `/api/vaults/files`, `/api/vaults/stats`, `/api/vaults/change_password`, `/api/vaults/recovery_key`, etc.).
- **Terminal User Interface (TUI)**: Volles TUI-Interface basierend auf `ratatui` und `crossterm` zur Verwaltung und Navigation im Terminal.
- **FUSE Dateisystem-Integration**: Erlaubt unter Linux/Unix das Einbinden des verschlüsselten Tresors direkt als virtuelles Verzeichnis im Dateisystem via Pure-Rust FUSE (ohne C `libfuse`-Bindung).
- **CLI-Befehle**: Vollwertige Steuerung ohne GUI über Subcommands (`list`, `create`, `mount`, `tui`, `serve`).
- **Sicherheitsarchitektur**:
  - **Schlüsselableitung**: Scrypt (N=16384, r=8, p=1) mit optionaler Unterstützung für Schlüsseldateien (Keyfiles).
  - **Dateiverschlüsselung**: Authenticated Encryption mit AES-256-GCM. Jede Datei enthält einen zufälligen 12-Byte-Nonce und verschlüsselte Pfadmetadaten.
  - **Dateinamen-Verschleierung**: Physikalische Dateinamen auf dem Datenträger werden deterministisch durch `SHA-256(master_key || relativer_pfad)` verschleiert.
  - **Auto-Lock**: Automatische Sperrung des entsperrten Tresors bei Inaktivität nach konfigurierbarer Zeit.
- **Code-Qualität & Compliance**:
  - 100 % Pure Rust: FUSE-Integration läuft nativ ohne `libfuse`-C-Bibliothek (`default-features = false`).
  - Strikte Zero-Panic-Policy via Clippy (`panic`, `unwrap_used`, `expect_used`, `panic_in_result_fn`, `unreachable`, `todo`, `unimplemented` auf `deny`).
  - Lizenzprüfung via [`deny.toml`](deny.toml) und automatisierter Integrationstest [`tests/test_license_compatibility.rs`](tests/test_license_compatibility.rs) zur Sicherstellung der MIT-Kompatibilität aller Abhängigkeiten.
  - Plattformübergreifend lauffähig (Linux X11/Wayland, macOS, Windows).

## Voraussetzungen & Installation

### Voraussetzungen (Linux)
Da die FUSE-Treiber-Implementierung 100 % in Pure Rust läuft, wird **keine C-Entwicklungsbibliothek (`libfuse3-dev`) zum Kompilieren benötigt**. Für unprivilegierte Desktop-Mounts und die GUI werden lediglich Grafikbibliotheken und das Systemwerkzeug `fuse3` (`fusermount3`) benötigt:

```bash
# Debian / Ubuntu
sudo apt install fuse3 pkg-config libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev

# Fedora
sudo dnf install fuse3 pkgconf-pkg-config libX11-devel libXcursor-devel libXrandr-devel libXi-devel mesa-libGL-devel

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
Für die intuitive Bedienung im Terminal:
```bash
cargo run --release -- tui
# Alternativ:
cargo run --release -- --tui
```
- `[↑/↓]` oder `[j/k]`: Tresor auswählen
- `[Enter]` oder `[u]`: Tresor mit Passwort entsperren & FUSE-Mount aktivieren
- `[n]`: Ausgewählten Tresor umbenennen
- `[d]`: Ausgewählten Tresor aus der Liste entfernen
- `[r]`: Tresor mit 64-stelligem Recovery-Key wiederherstellen & mounten
- `[x]`: Wiederherstellungsschlüssel des entsperrten Tresors anzeigen
- `[l]`: Tresor sperren & unmounten
- `[a]`: Bestehenden Tresorpfad registrieren
- `[c]`: Neuen Tresor interaktiv erstellen
- `[q]`: TUI beenden

### 3. Web-Client & Daemon (Syncthing-Stil)
Startet den Hintergrunddienst mit eingebettetem Web-Dashboard:
```bash
cargo run --release -- serve --host 127.0.0.1 --port 8080
# Alternativ:
cargo run --release -- --serve
```
Anschließend im Browser **`http://127.0.0.1:8080`** aufrufen, um Tresore zu verwalten, zu entsperren und Dateien per Drag & Drop zu übertragen.

### 4. CLI-Befehle
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

#### Passwort ändern
```bash
cargo run --release -- change-password --path "/pfad/zum/tresor"
# Oder mit Parametern:
cargo run --release -- change-password --path "/pfad/zum/tresor" --old-password "altes_pw" --new-password "neues_pw"
```

#### Wiederherstellungsschlüssel exportieren
```bash
cargo run --release -- recovery-key --path "/pfad/zum/tresor"
```

#### Tresor mit Wiederherstellungsschlüssel prüfen / entsperren
```bash
cargo run --release -- recover --path "/pfad/zum/tresor" --key "XXXX-XXXX-XXXX-..."
```

#### Dateien im Tresor auflisten (ls)
```bash
cargo run --release -- ls --path "/pfad/zum/tresor"
```

#### Datei in Tresor verschlüsseln & ablegen (put)
```bash
cargo run --release -- put --path "/pfad/zum/tresor" --source "lokale_datei.txt" --target "ordner/datei.txt"
```

#### Dateiinhalt entschlüsseln & ausgeben (cat)
```bash
cargo run --release -- cat --path "/pfad/zum/tresor" --file "ordner/datei.txt"
```

#### Datei im Tresor löschen (rm)
```bash
cargo run --release -- rm --path "/pfad/zum/tresor" --file "ordner/datei.txt"
```

#### Datei oder Ordner im Tresor verschieben / umbenennen (mv)
```bash
cargo run --release -- mv --path "/pfad/zum/tresor" --old-path "dokumente/alt.txt" --new-path "archiv/neu.txt"
```

#### Tresor-Statistiken abrufen (stats)
```bash
cargo run --release -- stats --path "/pfad/zum/tresor"
```

#### Verzeichnis im Tresor erstellen (mkdir)
```bash
cargo run --release -- mkdir --path "/pfad/zum/tresor" --folder "projekte/geheim"
```

#### Gesamten Tresor in lokales Verzeichnis exportieren (export)
```bash
cargo run --release -- export --path "/pfad/zum/tresor" --dest "/pfad/zum/zielordner"
```

#### Lokales Verzeichnis rekursiv in den Tresor importieren (import)
```bash
cargo run --release -- import --path "/pfad/zum/tresor" --source "/pfad/zur/quelle" --dest "backup"
```

## Projektstruktur

- [`src/main.rs`](src/main.rs): Einstiegspunkt, CLI-Argument-Parsing (`clap`) und Befehls-Dispatching.
- [`src/vault.rs`](src/vault.rs): Kryptografischer Kern (Scrypt KDF, AES-256-GCM, Pfad-Hashing, Tresor-Initialisierung & Dateiverschlüsselung).
- [`src/web_server.rs`](src/web_server.rs): Web-Server und REST-API via `axum` und `tokio`.
- [`src/web/index.html`](src/web/index.html): Eingebettetes, responsives Web-Dashboard (Single-Page-App).
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
