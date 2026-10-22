use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::fs_fuse;
use crate::vault;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SavedVault {
    name: String,
    path: String,
}

const CONFIG_FILE: &str = "vaults.json";

fn load_saved_vaults() -> Vec<SavedVault> {
    if Path::new(CONFIG_FILE).exists()
        && let Ok(content) = std::fs::read_to_string(CONFIG_FILE)
        && let Ok(vaults) = serde_json::from_str::<Vec<SavedVault>>(&content)
    {
        return vaults;
    }
    Vec::new()
}

fn save_saved_vaults(vaults: &[SavedVault]) {
    if let Ok(content) = serde_json::to_string_pretty(vaults) {
        let _ = std::fs::write(CONFIG_FILE, content);
    }
}

enum InputMode {
    Normal,
    PasswordInput,
    AddVaultPathInput,
    CreateNameInput,
    CreatePathInput,
    CreatePasswordInput,
}

struct TuiApp {
    vaults: Vec<SavedVault>,
    active_mounts: HashMap<PathBuf, (fuser::BackgroundSession, PathBuf)>, // path -> (session, mountpoint)
    list_state: ListState,
    input_mode: InputMode,
    input_value: String,
    status_message: String,
    status_is_error: bool,

    // Create Wizard state
    create_name: String,
    create_path: String,
}

impl TuiApp {
    fn new() -> Self {
        let vaults = load_saved_vaults();
        let mut list_state = ListState::default();
        if !vaults.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            vaults,
            active_mounts: HashMap::new(),
            list_state,
            input_mode: InputMode::Normal,
            input_value: String::new(),
            status_message: "Welcome to Cryptomator RS TUI!".to_string(),
            status_is_error: false,
            create_name: String::new(),
            create_path: String::new(),
        }
    }

    fn select_next(&mut self) {
        if self.vaults.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.vaults.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn select_prev(&mut self) {
        if self.vaults.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.vaults.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn unlock_active_vault(&mut self, password: &str) {
        let Some(idx) = self.list_state.selected() else {
            return;
        };
        let vault = self.vaults[idx].clone();
        let vault_path = PathBuf::from(&vault.path);

        match vault::unlock_vault(&vault_path, password, None) {
            Ok(master_key) => {
                // Setup and create FUSE mount directory in user home: ~/CryptomatorMounts/<vault_name>
                let home_dir =
                    std::env::var("HOME").unwrap_or_else(|_| "/home/adrichel".to_string());
                let mount_base = Path::new(&home_dir).join("CryptomatorMounts");
                let mount_dir = mount_base.join(&vault.name);

                if let Err(e) = std::fs::create_dir_all(&mount_dir) {
                    self.status_message = format!("Failed to create mount folder: {}", e);
                    self.status_is_error = true;
                    return;
                }

                let fs_impl = fs_fuse::CryptomatorFS::new(vault_path.clone(), master_key);
                let mount_options = vec![fuser::MountOption::CUSTOM(
                    "fsname=cryptomator-rs".to_string(),
                )];

                match fuser::spawn_mount2(fs_impl, &mount_dir, &mount_options) {
                    Ok(session) => {
                        self.active_mounts
                            .insert(vault_path, (session, mount_dir.clone()));
                        self.status_message = format!("Vault unlocked! Mounted at {:?}", mount_dir);
                        self.status_is_error = false;
                    }
                    Err(e) => {
                        self.status_message = format!("Mount failed: {}", e);
                        self.status_is_error = true;
                    }
                }
            }
            Err(e) => {
                self.status_message = format!("Unlock failed: {}", e);
                self.status_is_error = true;
            }
        }
    }

    fn lock_active_vault(&mut self) {
        let Some(idx) = self.list_state.selected() else {
            return;
        };
        let vault_path = PathBuf::from(&self.vaults[idx].path);
        if let Some((_, mountpoint)) = self.active_mounts.remove(&vault_path) {
            self.status_message = format!("Vault locked. Unmounted {:?}", mountpoint);
            self.status_is_error = false;
        } else {
            self.status_message = "Vault is not unlocked.".to_string();
            self.status_is_error = true;
        }
    }

    fn add_existing_vault(&mut self, path_str: &str) {
        let path = PathBuf::from(path_str.trim());
        let mk_path = path.join("masterkey.json");
        if !mk_path.exists() {
            self.status_message =
                "Selected directory is not a valid vault (masterkey.json missing)".to_string();
            self.status_is_error = true;
            return;
        }

        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Existing Vault".to_string());

        let clean_path = path.to_string_lossy().into_owned();

        if self.vaults.iter().any(|v| v.path == clean_path) {
            self.status_message = "Vault is already in the list.".to_string();
            self.status_is_error = true;
            return;
        }

        self.vaults.push(SavedVault {
            name: name.clone(),
            path: clean_path,
        });
        save_saved_vaults(&self.vaults);
        self.list_state.select(Some(self.vaults.len() - 1));
        self.status_message = format!("Vault '{}' added successfully.", name);
        self.status_is_error = false;
    }

    fn create_vault(&mut self, name: &str, path_str: &str, password: &str) {
        let path = Path::new(path_str.trim());
        match vault::create_vault(path, password, None) {
            Ok(_) => {
                self.vaults.push(SavedVault {
                    name: name.to_string(),
                    path: path_str.to_string(),
                });
                save_saved_vaults(&self.vaults);
                self.list_state.select(Some(self.vaults.len() - 1));
                self.status_message = format!("Vault '{}' created and registered!", name);
                self.status_is_error = false;
            }
            Err(e) => {
                self.status_message = format!("Failed to create vault: {}", e);
                self.status_is_error = true;
            }
        }
    }
}

pub fn run_tui() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = TuiApp::new();
    let res = run_loop(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error running TUI: {:?}", err);
    }

    Ok(())
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut TuiApp,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui_draw(f, app))?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            match app.input_mode {
                InputMode::Normal => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return Ok(());
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.select_next();
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.select_prev();
                    }
                    KeyCode::Char('u') | KeyCode::Enter => {
                        // Unlock/Lock toggle
                        if let Some(idx) = app.list_state.selected() {
                            let path = PathBuf::from(&app.vaults[idx].path);
                            if app.active_mounts.contains_key(&path) {
                                app.lock_active_vault();
                            } else {
                                app.input_mode = InputMode::PasswordInput;
                                app.input_value.clear();
                                app.status_message = "Enter vault password:".to_string();
                            }
                        }
                    }
                    KeyCode::Char('l') => {
                        app.lock_active_vault();
                    }
                    KeyCode::Char('a') => {
                        app.input_mode = InputMode::AddVaultPathInput;
                        app.input_value.clear();
                        app.status_message = "Enter path to existing vault folder:".to_string();
                    }
                    KeyCode::Char('c') => {
                        app.input_mode = InputMode::CreateNameInput;
                        app.input_value.clear();
                        app.status_message = "[Step 1/3] Enter name for new vault:".to_string();
                    }
                    _ => {}
                },
                InputMode::PasswordInput => match key.code {
                    KeyCode::Enter => {
                        let pwd = app.input_value.clone();
                        app.input_value.clear();
                        app.input_mode = InputMode::Normal;
                        app.unlock_active_vault(&pwd);
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.status_message = "Unlock cancelled.".to_string();
                    }
                    KeyCode::Char(c) => {
                        app.input_value.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input_value.pop();
                    }
                    _ => {}
                },
                InputMode::AddVaultPathInput => match key.code {
                    KeyCode::Enter => {
                        let path = app.input_value.clone();
                        app.input_value.clear();
                        app.input_mode = InputMode::Normal;
                        app.add_existing_vault(&path);
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.status_message = "Add vault cancelled.".to_string();
                    }
                    KeyCode::Char(c) => {
                        app.input_value.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input_value.pop();
                    }
                    _ => {}
                },
                InputMode::CreateNameInput => match key.code {
                    KeyCode::Enter => {
                        app.create_name = app.input_value.clone();
                        app.input_value.clear();
                        app.input_mode = InputMode::CreatePathInput;
                        app.status_message =
                            "[Step 2/3] Enter absolute directory path to create vault folder:"
                                .to_string();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.status_message = "Creation wizard cancelled.".to_string();
                    }
                    KeyCode::Char(c) => {
                        app.input_value.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input_value.pop();
                    }
                    _ => {}
                },
                InputMode::CreatePathInput => match key.code {
                    KeyCode::Enter => {
                        app.create_path = app.input_value.clone();
                        app.input_value.clear();
                        app.input_mode = InputMode::CreatePasswordInput;
                        app.status_message =
                            "[Step 3/3] Enter master password for the new vault:".to_string();
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.status_message = "Creation wizard cancelled.".to_string();
                    }
                    KeyCode::Char(c) => {
                        app.input_value.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input_value.pop();
                    }
                    _ => {}
                },
                InputMode::CreatePasswordInput => match key.code {
                    KeyCode::Enter => {
                        let pwd = app.input_value.clone();
                        app.input_value.clear();
                        app.input_mode = InputMode::Normal;
                        app.create_vault(&app.create_name.clone(), &app.create_path.clone(), &pwd);
                    }
                    KeyCode::Esc => {
                        app.input_mode = InputMode::Normal;
                        app.status_message = "Creation wizard cancelled.".to_string();
                    }
                    KeyCode::Char(c) => {
                        app.input_value.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input_value.pop();
                    }
                    _ => {}
                },
            }
        }
    }
}

fn ui_draw(f: &mut ratatui::Frame, app: &mut TuiApp) {
    // Premium theme colors
    let theme_cyan = Color::Rgb(0, 180, 216);
    let theme_dark_bg = Color::Rgb(15, 23, 42);
    let theme_card_bg = Color::Rgb(30, 41, 59);
    let theme_green = Color::Rgb(16, 185, 129);
    let theme_red = Color::Rgb(239, 68, 68);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title bar
            Constraint::Min(10),   // Main content (sidebar + detail panel)
            Constraint::Length(3), // Input panel (if active)
            Constraint::Length(3), // Status bar
        ])
        .split(f.size());

    // --- 1. Title Panel ---
    let title_block = Block::default()
        .style(Style::default().bg(theme_dark_bg))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme_cyan));
    let title_paragraph = Paragraph::new(Line::from(vec![
        Span::styled(
            "🛡️  CRYPTOMATOR RS  ",
            Style::default().fg(theme_cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Terminal Interface v0.1.0",
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(title_block);
    f.render_widget(title_paragraph, chunks[0]);

    // --- 2. Main Content Split ---
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33), // Sidebar
            Constraint::Percentage(67), // Detail Panel
        ])
        .split(chunks[1]);

    // Left: Vault List
    let list_items: Vec<ListItem> = app
        .vaults
        .iter()
        .map(|v| {
            let path = PathBuf::from(&v.path);
            let is_unlocked = app.active_mounts.contains_key(&path);

            let status_prefix = if is_unlocked {
                Span::styled(
                    "🔓 UNLOCKED ",
                    Style::default()
                        .fg(theme_green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    "🔒 LOCKED   ",
                    Style::default().fg(theme_red).add_modifier(Modifier::BOLD),
                )
            };

            let name = Span::styled(v.name.clone(), Style::default().fg(Color::White));

            ListItem::new(Line::from(vec![status_prefix, name]))
        })
        .collect();

    let sidebar_block = Block::default()
        .title(" Vaults ")
        .style(Style::default().bg(theme_dark_bg))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let sidebar_list = List::new(list_items).block(sidebar_block).highlight_style(
        Style::default()
            .bg(theme_card_bg)
            .add_modifier(Modifier::BOLD)
            .fg(theme_cyan),
    );
    f.render_stateful_widget(sidebar_list, main_layout[0], &mut app.list_state);

    // Right: Detailed Panel
    let detail_block = Block::default()
        .title(" Vault Details ")
        .style(Style::default().bg(theme_dark_bg))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let mut detail_lines = Vec::new();

    if let Some(idx) = app.list_state.selected() {
        let vault = &app.vaults[idx];
        let path = PathBuf::from(&vault.path);
        let mount_info = app.active_mounts.get(&path);
        let is_unlocked = mount_info.is_some();

        detail_lines.push(Line::from(vec![
            Span::styled("Name:         ", Style::default().fg(Color::Gray)),
            Span::styled(
                vault.name.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        detail_lines.push(Line::from(vec![
            Span::styled("Source Path:  ", Style::default().fg(Color::Gray)),
            Span::styled(vault.path.clone(), Style::default().fg(Color::LightBlue)),
        ]));

        detail_lines.push(Line::from(vec![
            Span::styled("Status:       ", Style::default().fg(Color::Gray)),
            if is_unlocked {
                Span::styled(
                    "UNLOCKED & MOUNTED",
                    Style::default()
                        .fg(theme_green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    "LOCKED (Encrypted at rest)",
                    Style::default().fg(theme_red).add_modifier(Modifier::BOLD),
                )
            },
        ]));

        if let Some((_, mountpoint)) = mount_info {
            detail_lines.push(Line::from(vec![
                Span::styled("Mountpoint:   ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{:?}", mountpoint),
                    Style::default().fg(theme_green),
                ),
            ]));

            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                "✔ You can access your decrypted files directly in this mount folder.",
                Style::default().fg(Color::DarkGray),
            )));
            detail_lines.push(Line::from(Span::styled(
                "✔ Use git inside this folder for client-side encrypted repository hosting.",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                "🔒 To access files or mount a git repository, press Enter to unlock.",
                Style::default().fg(Color::Yellow),
            )));
        }
    } else {
        detail_lines.push(Line::from(Span::styled(
            "No vaults added yet.",
            Style::default().fg(Color::Gray),
        )));
        detail_lines.push(Line::from(Span::styled(
            "Press 'a' to register an existing vault folder or 'c' to create a new one.",
            Style::default().fg(Color::Gray),
        )));
    }

    let detail_paragraph = Paragraph::new(detail_lines)
        .block(detail_block)
        .wrap(Wrap { trim: true });
    f.render_widget(detail_paragraph, main_layout[1]);

    // --- 3. Input Panel (Contextual modal style or inline) ---
    match app.input_mode {
        InputMode::Normal => {
            // Draw empty filler
            let empty_block = Block::default()
                .borders(Borders::NONE)
                .style(Style::default().bg(theme_dark_bg));
            f.render_widget(empty_block, chunks[2]);
        }
        _ => {
            let input_title = match app.input_mode {
                InputMode::PasswordInput => " Enter Password ",
                InputMode::AddVaultPathInput => " Path to Existing Vault ",
                InputMode::CreateNameInput => " New Vault Name (1/3) ",
                InputMode::CreatePathInput => " Base Folder Path (2/3) ",
                InputMode::CreatePasswordInput => " Set Password (3/3) ",
                InputMode::Normal => " Input ",
            };

            let input_block = Block::default()
                .title(input_title)
                .style(Style::default().bg(theme_card_bg))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme_cyan));

            // Hide password characters
            let display_val = if matches!(
                app.input_mode,
                InputMode::PasswordInput | InputMode::CreatePasswordInput
            ) {
                "*".repeat(app.input_value.len())
            } else {
                app.input_value.clone()
            };

            let input_paragraph = Paragraph::new(Line::from(vec![
                Span::styled("> ", Style::default().fg(theme_cyan)),
                Span::styled(display_val, Style::default().fg(Color::White)),
                Span::styled(
                    "_",
                    Style::default()
                        .fg(theme_cyan)
                        .add_modifier(Modifier::RAPID_BLINK),
                ),
            ]))
            .block(input_block);
            f.render_widget(input_paragraph, chunks[2]);
        }
    }

    // --- 4. Status Bar & Keybindings Panel ---
    let status_style = if app.status_is_error {
        Style::default().fg(theme_red).bg(theme_dark_bg)
    } else {
        Style::default().fg(Color::Gray).bg(theme_dark_bg)
    };

    let status_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(theme_dark_bg));

    let bindings_text = Span::styled(
        " [↑/↓] Select  [Enter/u] Toggle Unlock  [l] Lock  [a] Add Existing  [c] Create  [q] Quit",
        Style::default().fg(theme_cyan),
    );

    let status_line = Line::from(vec![
        Span::styled(format!(" Status: {}  | ", app.status_message), status_style),
        bindings_text,
    ]);

    let status_paragraph = Paragraph::new(status_line)
        .block(status_block)
        .wrap(Wrap { trim: true });
    f.render_widget(status_paragraph, chunks[3]);
}
