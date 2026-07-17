# Cryptomator RS (Rust & Slint GUI)

Eine moderne, speichersichere und performante Implementierung des Cryptomator-Konzepts in Rust mit einer Slint-basierten grafischen Benutzeroberfläche.

## Features

- **Erstellung neuer Tresore**: Generiert ein Salz, leitet einen Schlüssel mittels `Scrypt` ab und verschlüsselt den Masterkey via `AES-256-GCM`.
- **Tresor-Verwaltung**: Speichert die Liste der hinzugefügten Tresore persistent in `vaults.json`.
- **Echtzeit-Dateimanager**: Ermöglicht das Importieren, Exportieren, Erstellen und Löschen von verschlüsselten Dateien direkt über die GUI.
- **Sicherheitsarchitektur**:
  - **Schlüsselableitung**: Scrypt (N=16384, r=8, p=1).
  - **Dateiverschlüsselung**: AES-256-GCM. Jede Datei enthält einen zufälligen 12-Byte-Nonce und verschlüsselte Metadaten (Pfadname), um Dateistrukturen komplett zu verschleiern.
  - **Dateinamen-Verschleierung**: Die physikalischen Dateinamen auf der Festplatte werden durch `SHA-256(master_key || relativer_pfad)` deterministisch verschleiert.

## Projektstruktur

- [`src/main.rs`](file:///home/adrichel/develop/cryptomator_rs/src/main.rs): Anwendungssteuerung, Event-Loop und Verknüpfung der Slint-GUI-Callbacks mit der Krypto-Bibliothek.
- [`src/vault.rs`](file:///home/adrichel/develop/cryptomator_rs/src/vault.rs): Kryptografische Operationen, Tresor-Erstellung, Entsperrung sowie Verschlüsselungs-/Entschlüsselungspipelines für Dateien.
- [`ui/appwindow.slint`](file:///home/adrichel/develop/cryptomator_rs/ui/appwindow.slint): Die deklarative Slint UI-Spezifikation (Aesthetic Dark Mode).
- [`Cargo.toml`](file:///home/adrichel/develop/cryptomator_rs/Cargo.toml): Cargo-Konfigurationsdatei mit den Abhängigkeiten (`aes-gcm`, `scrypt`, `rfd`, `slint`).

## Starten der Anwendung

Führe im Ordner `cryptomator_rs` folgenden Befehl aus, um die Anwendung zu kompilieren und zu starten:

```bash
cargo run --release
```
