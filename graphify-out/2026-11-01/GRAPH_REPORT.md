# Graph Report - cryptomator_rs  (2026-11-01)

## Corpus Check
- 17 files · ~45,675 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 4 file(s) not represented in the graph (top: .toml 2, (none) 2)

## Summary
- 620 nodes · 1216 edges · 34 communities (21 shown, 13 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 29 edges (avg confidence: 0.86)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `567b15b2`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Core Cryptography & Vault Engine
- Axum Web Server & REST API
- FUSE Filesystem & POSIX Errors
- egui Graphical User Interface
- CLI Command Parser & Handlers
- Project Documentation & Overview
- Web Client Vault Operations API
- Ratatui Terminal UI Layout
- Integration & Workflow Tests
- Development Guidelines & AI Context
- TUI Application State & Events
- Application Lifecycle & Signals
- TUI Input Modes & Prompts
- Agent Instructions & Rules
- Source Code Signatures & Maps
- Web UI Vault Creation & Auth
- Gemini AI Context Signatures
- Vault Session Persistence
- System Diagnostics & Benchmarks
- OpenAI Context Signatures
- Web UI File Copy Operations
- Web UI File Deletion Handlers
- Web UI Fulltext Search
- Web UI Directory Creation
- Recovery Key Management
- egui Frame Rendering
- Web UI File Listing
- Web UI Vault Removal
- Web UI Item Renaming
- Web UI Vault Renaming
- Web UI Vault Statistics
- Crate Root Package

## God Nodes (most connected - your core abstractions)
1. `Commands` - 38 edges
2. `AppState` - 32 edges
3. `CryptomatorApp` - 30 edges
4. `4. CLI-Befehle` - 29 edges
5. `ErrorResponse` - 28 edges
6. `write_file()` - 22 edges
7. `CryptomatorFS` - 22 edges
8. `unlock_vault()` - 21 edges
9. `create_vault()` - 20 edges
10. `TuiApp` - 17 edges

## Surprising Connections (you probably didn't know these)
- `Source Code Project Structure` --references--> `SigMap Code Signatures`  [INFERRED]
  README.md → .github/context-src.md
- `create_or_register_vault_handler()` --calls--> `load_saved_vaults()`  [INFERRED]
  src/web_server.rs → src/main.rs
- `list_vaults_handler()` --calls--> `load_saved_vaults()`  [INFERRED]
  src/web_server.rs → src/main.rs
- `remove_vault_handler()` --calls--> `load_saved_vaults()`  [INFERRED]
  src/web_server.rs → src/main.rs
- `rename_vault_handler()` --calls--> `load_saved_vaults()`  [INFERRED]
  src/web_server.rs → src/main.rs

## Import Cycles
- None detected.

## Communities (34 total, 13 thin omitted)

### Community 0 - "Core Cryptography & Vault Engine"
Cohesion: 0.05
Nodes (85): BenchmarkResult, change_vault_password(), compute_unified_diff(), copy_dir(), copy_file(), create_dir(), create_vault(), days_to_civil() (+77 more)

### Community 1 - "Axum Web Server & REST API"
Cohesion: 0.08
Nodes (68): AddVaultRequest, AppState, backup_key_handler(), BackupKeyRequest, change_password_handler(), ChangePasswordRequest, copy_file_handler(), CopyFileRequest (+60 more)

### Community 2 - "FUSE Filesystem & POSIX Errors"
Cohesion: 0.07
Nodes (10): CryptomatorFS, FSNode, Directory, File, get_decrypted_file_size(), make_attr(), test_fuse_rename_recursive(), test_fuse_tree_construction_and_read() (+2 more)

### Community 3 - "egui Graphical User Interface"
Cohesion: 0.06
Nodes (12): CryptomatorApp, file_icon(), run_gui(), UnlockedTab, Files, Stats, Trash, ViewMode (+4 more)

### Community 4 - "CLI Command Parser & Handlers"
Cohesion: 0.05
Nodes (40): CliArgs, Commands, Add, BackupKey, Benchmark, Cat, ChangePassword, Completions (+32 more)

### Community 5 - "Project Documentation & Overview"
Cohesion: 0.05
Nodes (41): 1. Grafische Oberfläche (GUI), 2. Terminal User Interface (TUI), 3. Web-Client & Daemon (Syncthing-Stil), 4. CLI-Befehle, Bauen, Bestehenden Tresor registrieren (add), Cryptomator RS (Rust & egui / Ratatui / Web Daemon), Datei in Tresor verschlüsseln & ablegen (put) (+33 more)

### Community 6 - "Web Client Vault Operations API"
Cohesion: 0.05
Nodes (42): POST /api/vaults/backup_key, GET /api/vaults/diff, GET /api/vaults/download, GET /api/vaults/export_zip, GET /api/vaults, GET /api/vaults/hash, POST /api/vaults/lock, GET /api/vaults/preview (+34 more)

### Community 8 - "Integration & Workflow Tests"
Cohesion: 0.13
Nodes (13): CLI_TEST_LOCK, ALLOWED_LICENSES, eval_spdx_tokens(), is_license_allowed(), is_license_compatible_with_mit(), test_all_dependencies_have_mit_compatible_licenses(), Token, And (+5 more)

### Community 9 - "Development Guidelines & AI Context"
Cohesion: 0.14
Nodes (14): Build and Verification Commands, AGENTS Memory and Guidelines, Source Module Signatures, SigMap Code Signatures, GitHub Copilot Context Signatures, Gemini Context Signatures, OpenAI Context Signatures, Cryptomator RS CLI Subcommands (+6 more)

### Community 10 - "TUI Application State & Events"
Cohesion: 0.23
Nodes (3): run_loop(), save_saved_vaults(), TuiApp

### Community 11 - "Application Lifecycle & Signals"
Cohesion: 0.11
Nodes (20): CONFIG_FILE, format_bytes_cli(), load_saved_vaults(), main(), MOUNT_RUNNING, save_saved_vaults(), SavedVault, sig_unmount_handler() (+12 more)

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

### Community 17 - "Vault Session Persistence"
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

## Knowledge Gaps
- **100 isolated node(s):** `Tui`, `Serve`, `List`, `Add`, `Remove` (+95 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 262 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **13 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `CryptomatorApp` connect `egui Graphical User Interface` to `Core Cryptography & Vault Engine`?**
  _High betweenness centrality (0.045) - this node is a cross-community bridge._
- **What connects `Tui`, `Serve`, `List` to the rest of the system?**
  _100 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Core Cryptography & Vault Engine` be split into smaller, more focused modules?**
  _Cohesion score 0.050597460791635546 - nodes in this community are weakly interconnected._
- **Why does `InputMode` connect `TUI Input Modes & Prompts` to `TUI Application State & Events`, `Ratatui Terminal UI Layout`?**
  _High betweenness centrality (0.023) - this node is a cross-community bridge._
- **Should `Axum Web Server & REST API` be split into smaller, more focused modules?**
  _Cohesion score 0.0822884012539185 - nodes in this community are weakly interconnected._
- **Should `FUSE Filesystem & POSIX Errors` be split into smaller, more focused modules?**
  _Cohesion score 0.07205387205387205 - nodes in this community are weakly interconnected._
- **Should `egui Graphical User Interface` be split into smaller, more focused modules?**
  _Cohesion score 0.06127946127946128 - nodes in this community are weakly interconnected._