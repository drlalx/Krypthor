# Graph Report - krypthor  (2026-11-08)

## Corpus Check
- 23 files · ~54,267 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 4 file(s) not represented in the graph (top: .toml 2, (none) 2)

## Summary
- 729 nodes · 1427 edges · 44 communities (32 shown, 12 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS · INFERRED: 7 edges (avg confidence: 0.89)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `97aaccd3`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- vault.rs
- Axum Web Server & REST API
- FUSE Filesystem & POSIX Errors
- krypthor
- CLI Command Parser & Handlers
- Project Documentation & Overview
- Web Client Vault Operations API
- Ratatui Terminal UI Layout
- test_license_compatibility.rs
- Development Guidelines & AI Context
- TUI Application State & Events
- egui_ui.rs
- TUI Input Modes & Prompts
- Agent Instructions & Rules
- Source Code Signatures & Maps
- Web UI Vault Creation & Auth
- Gemini AI Context Signatures
- SavedVault
- System Diagnostics & Benchmarks
- OpenAI Context Signatures
- Web UI File Copy Operations
- Web UI File Deletion Handlers
- Web UI Fulltext Search
- Web UI Directory Creation
- Recovery Key Management
- Web UI File Listing
- Web UI Vault Removal
- Web UI Item Renaming
- Web UI Vault Renaming
- Web UI Vault Statistics
- unlock_vault
- decrypt_file_payload
- webdav_fs.rs
- test_trash_lifecycle
- CompressionMode
- list_files
- read_file
- list_files_with_details
- KrypthorApp
- lib.rs
- install-linux.sh
- install-macos.sh

## God Nodes (most connected - your core abstractions)
1. `Commands` - 40 edges
2. `AppState` - 34 edges
3. `KrypthorApp` - 31 edges
4. `ErrorResponse` - 29 edges
5. `4. CLI-Befehle` - 29 edges
6. `KrypthorFS` - 22 edges
7. `unlock_vault()` - 21 edges
8. `create_vault()` - 20 edges
9. `write_file()` - 20 edges
10. `GenericSuccessResponse` - 17 edges

## Surprising Connections (you probably didn't know these)
- `test_daemon_webdav_routing()` --calls--> `build_router()`  [INFERRED]
  tests/test_webdav.rs → src/web_server.rs
- `test_webdav_operations_in_memory()` --calls--> `create_webdav_handler()`  [INFERRED]
  tests/test_webdav.rs → src/webdav_fs.rs
- `Source Code Project Structure` --references--> `SigMap Code Signatures`  [INFERRED]
  README.md → .github/context-src.md
- `AGENTS Memory and Guidelines` --references--> `GitHub Copilot Context Signatures`  [EXTRACTED]
  AGENTS.md → .github/copilot-instructions.md
- `Gemini Context Signatures` --semantically_similar_to--> `GitHub Copilot Context Signatures`  [INFERRED] [semantically similar]
  .github/gemini-context.md → .github/copilot-instructions.md

## Import Cycles
- None detected.

## Communities (44 total, 12 thin omitted)

### Community 0 - "vault.rs"
Cohesion: 0.06
Nodes (27): BenchmarkResult, days_to_civil(), derive_key(), DoctorCheck, DoctorReport, export_recovery_key(), FileGrepResult, format_timestamp() (+19 more)

### Community 1 - "Axum Web Server & REST API"
Cohesion: 0.06
Nodes (86): AddVaultRequest, AppState, backup_key_handler(), BackupKeyRequest, build_router(), change_password_handler(), ChangePasswordRequest, copy_file_handler() (+78 more)

### Community 2 - "FUSE Filesystem & POSIX Errors"
Cohesion: 0.08
Nodes (10): FSNode, Directory, File, get_decrypted_file_size(), KrypthorFS, make_attr(), test_fuse_rename_recursive(), test_fuse_tree_construction_and_read() (+2 more)

### Community 4 - "CLI Command Parser & Handlers"
Cohesion: 0.05
Nodes (42): CliArgs, Commands, Add, BackupKey, Benchmark, Cat, ChangePassword, Completions (+34 more)

### Community 5 - "Project Documentation & Overview"
Cohesion: 0.05
Nodes (41): 1. Grafische Oberfläche (GUI), 2. Terminal User Interface (TUI), 3. Web-Client & Daemon (Syncthing-Stil), 4. CLI-Befehle, Bauen, Bestehenden Tresor registrieren (add), Cryptomator RS (Rust & egui / Ratatui / Web Daemon), Datei in Tresor verschlüsseln & ablegen (put) (+33 more)

### Community 6 - "Web Client Vault Operations API"
Cohesion: 0.05
Nodes (42): POST /api/vaults/backup_key, GET /api/vaults/diff, GET /api/vaults/download, GET /api/vaults/export_zip, GET /api/vaults, GET /api/vaults/hash, POST /api/vaults/lock, GET /api/vaults/preview (+34 more)

### Community 8 - "test_license_compatibility.rs"
Cohesion: 0.13
Nodes (13): CLI_TEST_LOCK, ALLOWED_LICENSES, eval_spdx_tokens(), is_license_allowed(), is_license_compatible_with_mit(), test_all_dependencies_have_mit_compatible_licenses(), Token, And (+5 more)

### Community 9 - "Development Guidelines & AI Context"
Cohesion: 0.14
Nodes (14): Build and Verification Commands, AGENTS Memory and Guidelines, Source Module Signatures, SigMap Code Signatures, GitHub Copilot Context Signatures, Gemini Context Signatures, OpenAI Context Signatures, Cryptomator RS CLI Subcommands (+6 more)

### Community 10 - "TUI Application State & Events"
Cohesion: 0.20
Nodes (4): run_loop(), save_saved_vaults(), TuiApp, ui_draw()

### Community 11 - "egui_ui.rs"
Cohesion: 0.05
Nodes (13): file_icon(), load_app_icon_rgba(), run_gui(), test_load_app_icon_rgba_valid(), ViewMode, CreateWizard, Locked, Unlocked (+5 more)

### Community 12 - "TUI Input Modes & Prompts"
Cohesion: 0.18
Nodes (11): InputMode, AddVaultPathInput, CreateKeyfileInput, CreateNameInput, CreatePasswordInput, CreatePathInput, Normal, PasswordInput (+3 more)

### Community 13 - "Agent Instructions & Rules"
Cohesion: 0.22
Nodes (7): AGENTS Memory - cryptomator_rs, Build & Test Befehle, Codebase Context & Signatures, Globale Arbeitsweise, Auto-generated signatures, Code signatures, Codebase overview

### Community 14 - "Source Code Signatures & Maps"
Cohesion: 0.22
Nodes (8): Code signatures, SigMap commands, src, src/cli.rs, src/fs_fuse.rs, src/fs.rs, src/tui_app.rs, src/vault.rs

### Community 15 - "Web UI Vault Creation & Auth"
Cohesion: 0.25
Nodes (8): POST /api/vaults/change_password, POST /api/vaults, handleAddVault(), handleChangePassword(), Add / Initialize Vault Modal, Change Vault Password Modal, Client-Side Secure Password Generator, Password Strength Evaluator

### Community 16 - "Gemini AI Context Signatures"
Cohesion: 0.50
Nodes (3): Auto-generated signatures, Code Signatures, Codebase overview

### Community 17 - "SavedVault"
Cohesion: 0.50
Nodes (3): load_saved_vaults(), run_tui(), SavedVault

### Community 18 - "System Diagnostics & Benchmarks"
Cohesion: 0.50
Nodes (4): GET /api/system/benchmark, GET /api/system/doctor, runSystemDoctor(), System Diagnostics & Benchmark Modal

### Community 22 - "Web UI Fulltext Search"
Cohesion: 0.67
Nodes (3): GET /api/vaults/grep, executeVaultGrep(), Fulltext Search (Grep) Modal

### Community 23 - "Web UI Directory Creation"
Cohesion: 0.67
Nodes (3): POST /api/vaults/mkdir, handleCreateFolder(), Create Directory Modal

### Community 24 - "Recovery Key Management"
Cohesion: 0.67
Nodes (3): POST /api/vaults/recovery_key, openRecoveryKeyModal(), Show Recovery Key Modal

### Community 31 - "unlock_vault"
Cohesion: 0.25
Nodes (20): change_vault_password(), copy_dir(), copy_file(), create_dir(), create_vault(), derive_final_key(), test_change_vault_password(), test_copy_file_and_dir() (+12 more)

### Community 32 - "decrypt_file_payload"
Cohesion: 0.14
Nodes (14): CompressionAlgorithm, None, Zstd, decrypt_file_payload(), encrypt_file_payload(), encrypt_file_payload_with_compression(), get_vault_stats(), IntegrityReport (+6 more)

### Community 33 - "webdav_fs.rs"
Cohesion: 0.08
Nodes (8): clean_rel_path(), create_webdav_handler(), KrypthorDavDirEntry, KrypthorDavFile, KrypthorDavFs, KrypthorDavMetaData, run_webdav_server(), test_webdav_operations_in_memory()

### Community 34 - "test_trash_lifecycle"
Cohesion: 0.21
Nodes (14): delete_file(), empty_trash(), get_physical_filename(), list_trash(), move_dir_to_trash(), move_to_trash(), prune_trash(), rename_dir() (+6 more)

### Community 35 - "CompressionMode"
Cohesion: 0.24
Nodes (9): CompressionMode, Always, Auto, Never, import_dir(), import_dir_with_compression(), write_file_from_reader(), write_file_from_reader_with_compression() (+1 more)

### Community 36 - "list_files"
Cohesion: 0.25
Nodes (8): delete_dir(), export_vault(), export_vault_zip(), FileHashResult, hash_file(), list_files(), read_file_to_writer(), test_create_dir_import_export()

### Community 37 - "read_file"
Cohesion: 0.48
Nodes (7): compute_unified_diff(), diff_vault_files(), diff_vault_with_local(), DiffChunk, DiffResult, read_file(), test_diff_vault_files_and_local()

### Community 38 - "list_files_with_details"
Cohesion: 0.83
Nodes (4): FileItem, find_files(), list_files_with_details(), test_file_metadata_and_find()

### Community 39 - "KrypthorApp"
Cohesion: 0.17
Nodes (5): KrypthorApp, UnlockedTab, Files, Stats, Trash

### Community 40 - "lib.rs"
Cohesion: 0.38
Nodes (4): CONFIG_FILE, load_saved_vaults(), save_saved_vaults(), SavedVault

### Community 41 - "install-linux.sh"
Cohesion: 0.62
Nodes (6): print_error(), print_info(), print_success(), print_warn(), install-linux.sh script, uninstall()

### Community 42 - "install-macos.sh"
Cohesion: 0.57
Nodes (5): print_error(), print_info(), print_success(), install-macos.sh script, uninstall()

## Knowledge Gaps
- **107 isolated node(s):** `Files`, `Stats`, `Trash`, `CreateWizard`, `Locked` (+102 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 286 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **12 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Commands` connect `CLI Command Parser & Handlers` to `CompressionMode`?**
  _High betweenness centrality (0.073) - this node is a cross-community bridge._
- **What connects `Files`, `Stats`, `Trash` to the rest of the system?**
  _107 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `vault.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06060606060606061 - nodes in this community are weakly interconnected._
- **Why does `CompressionMode` connect `CompressionMode` to `vault.rs`, `decrypt_file_payload`, `CLI Command Parser & Handlers`?**
  _High betweenness centrality (0.054) - this node is a cross-community bridge._
- **Should `Axum Web Server & REST API` be split into smaller, more focused modules?**
  _Cohesion score 0.06334371754932502 - nodes in this community are weakly interconnected._
- **Why does `KrypthorApp` connect `KrypthorApp` to `vault.rs`, `test_trash_lifecycle`, `read_file`, `list_files_with_details`, `egui_ui.rs`?**
  _High betweenness centrality (0.046) - this node is a cross-community bridge._
- **Should `FUSE Filesystem & POSIX Errors` be split into smaller, more focused modules?**
  _Cohesion score 0.0784313725490196 - nodes in this community are weakly interconnected._