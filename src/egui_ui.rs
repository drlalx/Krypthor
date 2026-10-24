#[cfg(feature = "egui-ui")]
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, Stroke, Vec2};
use elegance::{Accent, Button, Card, Theme};

use crate::vault;
use crate::{SavedVault, load_saved_vaults, save_saved_vaults};

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum ViewMode {
    Welcome,
    CreateWizard,
    Locked,
    Unlocked,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum UnlockedTab {
    Files,
    Stats,
}

#[cfg(feature = "egui-ui")]
pub fn run_gui() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([760.0, 520.0])
            .with_title("Cryptomator RS - Rust Edition"),
        ..Default::default()
    };

    eframe::run_native(
        "Cryptomator RS",
        options,
        Box::new(|_cc| Ok(Box::new(CryptomatorApp::new()))),
    )
}

#[cfg(feature = "egui-ui")]
struct CryptomatorApp {
    saved_vaults: Vec<SavedVault>,
    selected_vault_idx: Option<usize>,
    unlocked_vaults: HashMap<PathBuf, [u8; 32]>,
    #[cfg(unix)]
    active_mounts: HashMap<PathBuf, (fuser::BackgroundSession, PathBuf)>,
    view_mode: ViewMode,
    unlocked_tab: UnlockedTab,
    status_message: Option<(String, bool)>, // (message, is_error)

    // Unlock form
    unlock_password: String,
    unlock_keyfile_path: Option<PathBuf>,
    auto_lock_enabled: bool,
    auto_lock_timeout_mins: u32,
    last_interaction: Instant,

    // Create wizard form
    create_name: String,
    create_path: String,
    create_password: String,
    create_password_confirm: String,
    create_use_keyfile: bool,
    create_keyfile_path: Option<PathBuf>,

    // File manager inside unlocked vault
    vault_files: Vec<vault::FileItem>,
    file_search_query: String,
    new_file_modal_open: bool,
    new_file_name: String,
    new_file_content: String,
    new_folder_modal_open: bool,
    new_folder_name: String,
    rename_modal_open: bool,
    rename_target: String,
    rename_new_name: String,

    // Password change & recovery modals
    change_pw_modal_open: bool,
    change_pw_old: String,
    change_pw_new: String,
    change_pw_confirm: String,
    change_pw_old_keyfile: Option<PathBuf>,
    change_pw_new_keyfile: Option<PathBuf>,

    show_recovery_key_modal_open: bool,
    displayed_recovery_key: String,

    recover_modal_open: bool,
    recover_key_input: String,

    // Vault rename modal
    rename_vault_modal_open: bool,
    rename_vault_new_name: String,

    // File preview modal
    preview_modal_open: bool,
    preview_file_name: String,
    preview_file_text: String,

    // Throughput & activity stats
    throughput_history: VecDeque<f32>,
    active_read_speed: f32,
    active_write_speed: f32,
    last_throughput_update: Instant,

    #[cfg(feature = "web")]
    web_server_running: bool,
    #[cfg(feature = "web")]
    web_server_shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

#[cfg(feature = "egui-ui")]
impl CryptomatorApp {
    fn new() -> Self {
        let saved_vaults = load_saved_vaults();
        let selected_vault_idx = if saved_vaults.is_empty() {
            None
        } else {
            Some(0)
        };
        let view_mode = if selected_vault_idx.is_some() {
            ViewMode::Locked
        } else {
            ViewMode::Welcome
        };

        let mut throughput_history = VecDeque::with_capacity(30);
        for _ in 0..30 {
            throughput_history.push_back(0.0);
        }

        Self {
            saved_vaults,
            selected_vault_idx,
            unlocked_vaults: HashMap::new(),
            #[cfg(unix)]
            active_mounts: HashMap::new(),
            view_mode,
            unlocked_tab: UnlockedTab::Files,
            status_message: None,

            unlock_password: String::new(),
            unlock_keyfile_path: None,
            auto_lock_enabled: false,
            auto_lock_timeout_mins: 10,
            last_interaction: Instant::now(),

            create_name: String::new(),
            create_path: String::new(),
            create_password: String::new(),
            create_password_confirm: String::new(),
            create_use_keyfile: false,
            create_keyfile_path: None,

            vault_files: Vec::new(),
            file_search_query: String::new(),
            new_file_modal_open: false,
            new_file_name: String::new(),
            new_file_content: String::new(),
            new_folder_modal_open: false,
            new_folder_name: String::new(),
            rename_modal_open: false,
            rename_target: String::new(),
            rename_new_name: String::new(),

            change_pw_modal_open: false,
            change_pw_old: String::new(),
            change_pw_new: String::new(),
            change_pw_confirm: String::new(),
            change_pw_old_keyfile: None,
            change_pw_new_keyfile: None,

            show_recovery_key_modal_open: false,
            displayed_recovery_key: String::new(),

            recover_modal_open: false,
            recover_key_input: String::new(),

            rename_vault_modal_open: false,
            rename_vault_new_name: String::new(),

            preview_modal_open: false,
            preview_file_name: String::new(),
            preview_file_text: String::new(),

            throughput_history,
            active_read_speed: 0.0,
            active_write_speed: 0.0,
            last_throughput_update: Instant::now(),

            #[cfg(feature = "web")]
            web_server_running: false,
            #[cfg(feature = "web")]
            web_server_shutdown: None,
        }
    }

    fn select_vault(&mut self, idx: usize) {
        if idx >= self.saved_vaults.len() {
            return;
        }
        self.selected_vault_idx = Some(idx);
        self.unlock_password.clear();
        self.unlock_keyfile_path = None;
        self.status_message = None;

        let path = PathBuf::from(&self.saved_vaults[idx].path);
        if self.unlocked_vaults.contains_key(&path) {
            self.view_mode = ViewMode::Unlocked;
            self.refresh_vault_files();
        } else {
            self.view_mode = ViewMode::Locked;
        }
    }

    fn lock_active_vault(&mut self) {
        if let Some(idx) = self.selected_vault_idx
            && let Some(vault) = self.saved_vaults.get(idx)
        {
            let path = PathBuf::from(&vault.path);
            #[cfg(unix)]
            {
                self.active_mounts.remove(&path);
            }
            self.unlocked_vaults.remove(&path);
            self.view_mode = ViewMode::Locked;
            self.vault_files.clear();
            self.status_message = Some(("Vault locked successfully.".to_string(), false));
        }
    }

    fn refresh_vault_files(&mut self) {
        if let Some(idx) = self.selected_vault_idx
            && let Some(vault) = self.saved_vaults.get(idx)
        {
            let path = PathBuf::from(&vault.path);
            if let Some(key) = self.unlocked_vaults.get(&path) {
                match vault::list_files_with_details(&path, key) {
                    Ok(files) => {
                        self.vault_files = files;
                    }
                    Err(e) => {
                        self.status_message = Some((format!("Error listing files: {e}"), true));
                    }
                }
            }
        }
    }

    fn update_throughput(&mut self) {
        if self.last_throughput_update.elapsed() >= Duration::from_millis(500) {
            self.last_throughput_update = Instant::now();

            // Decay active speeds
            if self.active_read_speed > 0.1 {
                self.active_read_speed *= 0.6;
            } else {
                self.active_read_speed = 0.0;
            }

            if self.active_write_speed > 0.1 {
                self.active_write_speed *= 0.6;
            } else {
                self.active_write_speed = 0.0;
            }

            let next_point = if self.active_read_speed > 0.1 || self.active_write_speed > 0.1 {
                (self.active_read_speed + self.active_write_speed).min(100.0)
            } else if self.view_mode == ViewMode::Unlocked {
                let secs = Instant::now()
                    .duration_since(self.last_interaction)
                    .as_secs_f32();
                (secs * 2.0).sin().abs() * 5.0 + 2.0
            } else {
                0.0
            };

            if self.throughput_history.len() >= 30 {
                self.throughput_history.pop_front();
            }
            self.throughput_history.push_back(next_point);
        }
    }

    fn check_auto_lock(&mut self) {
        if self.auto_lock_enabled && self.view_mode == ViewMode::Unlocked {
            let timeout = Duration::from_secs(self.auto_lock_timeout_mins as u64 * 60);
            if self.last_interaction.elapsed() >= timeout {
                self.lock_active_vault();
                self.status_message =
                    Some(("Vault auto-locked due to inactivity.".to_string(), false));
            }
        }
    }
}

#[cfg(feature = "egui-ui")]
impl eframe::App for CryptomatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        Theme::slate().install(ui.ctx());

        // Record interaction for auto-lock
        let has_input = ui
            .ctx()
            .input(|i| !i.events.is_empty() || i.pointer.any_click());
        if has_input {
            self.last_interaction = Instant::now();
        }

        self.update_throughput();
        self.check_auto_lock();
        ui.ctx().request_repaint_after(Duration::from_millis(500));

        egui::CentralPanel::default().show(ui, |ui| {
            // Top Status Notification Banner (if any)
            let mut clear_status = false;
            if let Some((ref msg, is_err)) = self.status_message {
                let bg_color = if is_err {
                    Color32::from_rgb(90, 24, 30)
                } else {
                    Color32::from_rgb(25, 75, 45)
                };
                let icon = if is_err { "⚠️" } else { "✓" };
                let msg_text = format!("{icon} {msg}");
                egui::Frame::new()
                    .fill(bg_color)
                    .corner_radius(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(msg_text).color(Color32::WHITE).strong());
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.button("✕").clicked() {
                                    clear_status = true;
                                }
                            });
                        });
                    });
                ui.add_space(8.0);
            }
            if clear_status {
                self.status_message = None;
            }

            // Main Two-Column Layout (Sidebar + Content)
            ui.horizontal(|ui| {
                // Left Column: Sidebar
                ui.allocate_ui_with_layout(
                    Vec2::new(260.0, ui.available_height()),
                    Layout::top_down(Align::Min),
                    |ui| {
                        self.render_sidebar(ui);
                    },
                );

                ui.separator();

                // Right Column: Active View
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), ui.available_height()),
                    Layout::top_down(Align::Min),
                    |ui| match self.view_mode {
                        ViewMode::Welcome => {
                            self.render_welcome_view(ui);
                        }
                        ViewMode::CreateWizard => {
                            self.render_create_wizard(ui);
                        }
                        ViewMode::Locked => {
                            self.render_locked_view(ui);
                        }
                        ViewMode::Unlocked => {
                            self.render_unlocked_view(ui);
                        }
                    },
                );
            });
        });
    }
}

#[cfg(feature = "egui-ui")]
impl CryptomatorApp {
    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("🔒").size(24.0));
            ui.vertical(|ui| {
                ui.label(RichText::new("Cryptomator RS").strong().size(16.0));
                ui.label(
                    RichText::new("Rust & egui Vaults")
                        .color(Color32::from_rgb(140, 150, 175))
                        .size(11.0),
                );
            });
        });

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui
                .add(Button::new("➕ New Vault").accent(Accent::Blue))
                .clicked()
            {
                self.view_mode = ViewMode::CreateWizard;
                self.status_message = None;
            }
            if ui.add(Button::new("📂 Add Existing")).clicked()
                && let Some(folder) = rfd::FileDialog::new().pick_folder()
            {
                let path_str = folder.to_string_lossy().to_string();
                let default_name = folder
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Existing Vault".to_string());
                let new_v = SavedVault {
                    name: default_name,
                    path: path_str,
                };
                self.saved_vaults.push(new_v);
                save_saved_vaults(&self.saved_vaults);
                let new_idx = self.saved_vaults.len() - 1;
                self.select_vault(new_idx);
                self.status_message = Some(("Vault added successfully.".to_string(), false));
            }
        });

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(
            RichText::new("REGISTERED VAULTS")
                .size(11.0)
                .color(Color32::from_rgb(120, 130, 155))
                .strong(),
        );
        ui.add_space(4.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.saved_vaults.is_empty() {
                ui.add_space(10.0);
                ui.label(
                    RichText::new("No registered vaults.")
                        .italics()
                        .color(Color32::GRAY),
                );
            } else {
                let mut to_delete = None;
                let mut to_select = None;
                let count = self.saved_vaults.len();
                for i in 0..count {
                    let (name, path_str) = match self.saved_vaults.get(i) {
                        Some(v) => (v.name.clone(), v.path.clone()),
                        None => continue,
                    };
                    let is_selected = self.selected_vault_idx == Some(i);
                    let path_buf = PathBuf::from(&path_str);
                    let is_unlocked = self.unlocked_vaults.contains_key(&path_buf);

                    let item_frame = egui::Frame::new()
                        .fill(if is_selected {
                            Color32::from_rgb(45, 55, 75)
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(6.0)
                        .inner_margin(8.0);

                    item_frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let status_icon = if is_unlocked { "🔓" } else { "🔒" };
                            ui.label(RichText::new(status_icon).size(15.0));

                            ui.vertical(|ui| {
                                ui.label(RichText::new(&name).strong());
                                ui.label(
                                    RichText::new(&path_str)
                                        .size(10.0)
                                        .color(Color32::from_rgb(140, 150, 175)),
                                );
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if is_selected {
                                    if ui
                                        .small_button("✕")
                                        .on_hover_text("Remove from list")
                                        .clicked()
                                    {
                                        to_delete = Some(i);
                                    }
                                    if ui.small_button("✏").on_hover_text("Rename vault").clicked()
                                    {
                                        self.rename_vault_modal_open = true;
                                        self.rename_vault_new_name = name.clone();
                                    }
                                }
                            });
                        });
                    });

                    let rect = ui.min_rect();
                    if ui.interact(rect, ui.id().with(i), Sense::click()).clicked() {
                        to_select = Some(i);
                    }
                    ui.add_space(2.0);
                }

                if let Some(sel) = to_select {
                    self.select_vault(sel);
                }

                if let Some(del_idx) = to_delete {
                    let path = PathBuf::from(&self.saved_vaults[del_idx].path);
                    #[cfg(unix)]
                    {
                        self.active_mounts.remove(&path);
                    }
                    self.unlocked_vaults.remove(&path);
                    self.saved_vaults.remove(del_idx);
                    save_saved_vaults(&self.saved_vaults);
                    if self.saved_vaults.is_empty() {
                        self.selected_vault_idx = None;
                        self.view_mode = ViewMode::Welcome;
                    } else {
                        self.select_vault(0);
                    }
                }

                if self.rename_vault_modal_open {
                    ui.add_space(8.0);
                    Card::new().heading("Rename Vault").show(ui, |ui| {
                        ui.label("New vault name:");
                        ui.text_edit_singleline(&mut self.rename_vault_new_name);
                        ui.horizontal(|ui| {
                            if ui.button("💾 Save").clicked() {
                                let trimmed = self.rename_vault_new_name.trim().to_string();
                                if !trimmed.is_empty() {
                                    if let Some(idx) = self.selected_vault_idx
                                        && let Some(v) = self.saved_vaults.get_mut(idx)
                                    {
                                        v.name = trimmed;
                                        save_saved_vaults(&self.saved_vaults);
                                        self.status_message = Some((
                                            "Vault renamed successfully.".to_string(),
                                            false,
                                        ));
                                    }
                                    self.rename_vault_modal_open = false;
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                self.rename_vault_modal_open = false;
                            }
                        });
                    });
                }
            }

            #[cfg(feature = "web")]
            {
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🌐").size(13.0));
                    let status_text = if self.web_server_running {
                        "Web Server (Aktiv)"
                    } else {
                        "Web Server (Aus)"
                    };
                    ui.label(RichText::new(status_text).size(11.0).strong());

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.web_server_running {
                            if ui.button("⏹ Stop").clicked() {
                                if let Some(tx) = self.web_server_shutdown.take() {
                                    let _ = tx.send(());
                                }
                                self.web_server_running = false;
                                self.status_message = Some(("Web server stopped.".into(), false));
                            }
                            ui.hyperlink_to("Öffnen ↗", "http://127.0.0.1:8080");
                        } else if ui.button("▶ Starten").clicked() {
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            self.web_server_shutdown = Some(tx);
                            self.web_server_running = true;
                            std::thread::spawn(move || {
                                if let Ok(rt) = tokio::runtime::Runtime::new() {
                                    let _ =
                                        rt.block_on(crate::web_server::run_server_with_shutdown(
                                            "127.0.0.1",
                                            8080,
                                            rx,
                                        ));
                                }
                            });
                            self.status_message =
                                Some(("Web server started on http://127.0.0.1:8080".into(), false));
                        }
                    });
                });
            }
        });
    }

    fn render_welcome_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(20.0);
        Card::new().heading("Welcome to Cryptomator RS").show(ui, |ui| {
            ui.label(RichText::new("Secure client-side zero-knowledge encryption in pure Rust with an elegant egui desktop interface.").size(14.0));
            ui.add_space(16.0);

            ui.label(RichText::new("Key Features:").strong());
            ui.label("• Scrypt key derivation (N=16384, r=8, p=1)");
            ui.label("• Authenticated encryption via AES-256-GCM");
            ui.label("• Fully obfuscated filenames through SHA-256 hashing");
            ui.label("• Optional keyfile support for two-factor vault security");
            #[cfg(unix)]
            ui.label("• Native FUSE filesystem mounting support");
            ui.label("• Real-time throughput and activity monitoring");

            ui.add_space(20.0);
            ui.horizontal(|ui| {
                if ui.add(Button::new("➕ Create New Vault").accent(Accent::Blue)).clicked() {
                    self.view_mode = ViewMode::CreateWizard;
                }
                if ui.add(Button::new("📂 Add Existing Vault")).clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        let path_str = folder.to_string_lossy().to_string();
                        let default_name = folder
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Existing Vault".to_string());
                        self.saved_vaults.push(SavedVault {
                            name: default_name,
                            path: path_str,
                        });
                        save_saved_vaults(&self.saved_vaults);
                        let idx = self.saved_vaults.len() - 1;
                        self.select_vault(idx);
                    }
            });
        });
    }

    fn render_create_wizard(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        Card::new().heading("Create New Vault").show(ui, |ui| {
            ui.label(
                RichText::new("Configure your new encrypted vault.")
                    .size(13.0)
                    .color(Color32::from_rgb(160, 170, 190)),
            );
            ui.add_space(16.0);

            ui.label(RichText::new("Vault Name").strong());
            ui.add(
                egui::TextEdit::singleline(&mut self.create_name)
                    .hint_text("e.g. Personal Documents"),
            );

            ui.add_space(10.0);
            ui.label(RichText::new("Vault Location").strong());
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.create_path)
                        .hint_text("/path/to/vault_folder"),
                );
                if ui.button("Browse...").clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    self.create_path = folder.to_string_lossy().to_string();
                }
            });

            ui.add_space(10.0);
            ui.label(RichText::new("Master Password").strong());
            ui.add(egui::TextEdit::singleline(&mut self.create_password).password(true));

            ui.add_space(10.0);
            ui.label(RichText::new("Confirm Password").strong());
            ui.add(egui::TextEdit::singleline(&mut self.create_password_confirm).password(true));

            ui.add_space(12.0);
            ui.checkbox(
                &mut self.create_use_keyfile,
                "Require Keyfile for extra security",
            );
            if self.create_use_keyfile {
                ui.horizontal(|ui| {
                    let keyfile_display = match &self.create_keyfile_path {
                        Some(p) => p.to_string_lossy().to_string(),
                        None => "No keyfile selected".to_string(),
                    };
                    ui.label(
                        RichText::new(keyfile_display)
                            .italics()
                            .color(Color32::LIGHT_BLUE),
                    );
                    if ui.button("Select Keyfile...").clicked()
                        && let Some(file) = rfd::FileDialog::new().pick_file()
                    {
                        self.create_keyfile_path = Some(file);
                    }
                });
            }

            ui.add_space(20.0);
            ui.horizontal(|ui| {
                if ui
                    .add(Button::new("Create Vault").accent(Accent::Green))
                    .clicked()
                {
                    if self.create_name.trim().is_empty() {
                        self.status_message =
                            Some(("Vault name cannot be empty.".to_string(), true));
                    } else if self.create_path.trim().is_empty() {
                        self.status_message =
                            Some(("Vault location cannot be empty.".to_string(), true));
                    } else if self.create_password.is_empty() {
                        self.status_message = Some(("Password cannot be empty.".to_string(), true));
                    } else if self.create_password != self.create_password_confirm {
                        self.status_message = Some(("Passwords do not match.".to_string(), true));
                    } else if self.create_use_keyfile && self.create_keyfile_path.is_none() {
                        self.status_message = Some((
                            "Please select a keyfile or uncheck the keyfile option.".to_string(),
                            true,
                        ));
                    } else {
                        let path_buf = PathBuf::from(&self.create_path);
                        let keyfile_ref = self.create_keyfile_path.as_deref();
                        match vault::create_vault(&path_buf, &self.create_password, keyfile_ref) {
                            Ok(_) => {
                                let new_v = SavedVault {
                                    name: self.create_name.trim().to_string(),
                                    path: self.create_path.trim().to_string(),
                                };
                                self.saved_vaults.push(new_v);
                                save_saved_vaults(&self.saved_vaults);
                                let new_idx = self.saved_vaults.len() - 1;
                                self.create_name.clear();
                                self.create_path.clear();
                                self.create_password.clear();
                                self.create_password_confirm.clear();
                                self.create_use_keyfile = false;
                                self.create_keyfile_path = None;
                                self.select_vault(new_idx);
                                self.status_message =
                                    Some(("Vault created successfully!".to_string(), false));
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Error creating vault: {e}"), true));
                            }
                        }
                    }
                }

                if ui.button("Cancel").clicked() {
                    if self.selected_vault_idx.is_some() {
                        self.view_mode = ViewMode::Locked;
                    } else {
                        self.view_mode = ViewMode::Welcome;
                    }
                }
            });
        });
    }

    fn render_locked_view(&mut self, ui: &mut egui::Ui) {
        let (vault_name, vault_path) = match self
            .selected_vault_idx
            .and_then(|i| self.saved_vaults.get(i))
        {
            Some(v) => (v.name.clone(), v.path.clone()),
            None => return,
        };

        ui.add_space(10.0);
        Card::new().heading(&vault_name).show(ui, |ui| {
            ui.label(
                RichText::new(&vault_path)
                    .size(12.0)
                    .color(Color32::from_rgb(140, 150, 175)),
            );
            ui.add_space(16.0);

            ui.label(RichText::new("Password").strong());
            let password_edit =
                ui.add(egui::TextEdit::singleline(&mut self.unlock_password).password(true));
            if password_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.trigger_unlock(&vault_path);
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let keyfile_str = match &self.unlock_keyfile_path {
                    Some(p) => p.to_string_lossy().to_string(),
                    None => "No keyfile selected (optional)".to_string(),
                };
                ui.label(RichText::new(keyfile_str).size(12.0).italics());
                if ui.button("Select Keyfile...").clicked()
                    && let Some(file) = rfd::FileDialog::new().pick_file()
                {
                    self.unlock_keyfile_path = Some(file);
                }
                if self.unlock_keyfile_path.is_some() && ui.button("Clear").clicked() {
                    self.unlock_keyfile_path = None;
                }
            });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.auto_lock_enabled, "Auto-lock after inactivity");
                if self.auto_lock_enabled {
                    ui.add(
                        egui::DragValue::new(&mut self.auto_lock_timeout_mins)
                            .range(1..=120)
                            .suffix(" min"),
                    );
                }
            });

            ui.add_space(20.0);
            ui.horizontal(|ui| {
                if ui
                    .add(Button::new("🔓 Unlock Vault").accent(Accent::Blue))
                    .clicked()
                {
                    self.trigger_unlock(&vault_path);
                }

                if ui.button("🔑 Recover with Key").clicked() {
                    self.recover_modal_open = true;
                    self.recover_key_input.clear();
                }
            });
        });

        if self.recover_modal_open {
            ui.add_space(10.0);
            Card::new()
                .heading("Recover Vault with Key")
                .show(ui, |ui| {
                    ui.label("Enter the 64-character recovery key (with or without dashes):");
                    ui.text_edit_singleline(&mut self.recover_key_input);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(Button::new("🔓 Recover & Unlock").accent(Accent::Green))
                            .clicked()
                        {
                            let path = PathBuf::from(&vault_path);
                            match vault::unlock_vault_with_recovery_key(&self.recover_key_input) {
                                Ok(key) => match vault::list_files(&path, &key) {
                                    Ok(_) => {
                                        self.unlocked_vaults.insert(path, key);
                                        self.view_mode = ViewMode::Unlocked;
                                        self.recover_modal_open = false;
                                        self.recover_key_input.clear();
                                        self.status_message = Some((
                                            "Vault recovered and unlocked!".to_string(),
                                            false,
                                        ));
                                        self.refresh_vault_files();
                                    }
                                    Err(e) => {
                                        self.status_message = Some((
                                            format!("Recovery key failed to decrypt: {e}"),
                                            true,
                                        ));
                                    }
                                },
                                Err(e) => {
                                    self.status_message =
                                        Some((format!("Invalid recovery key: {e}"), true));
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.recover_modal_open = false;
                        }
                    });
                });
        }
    }

    fn trigger_unlock(&mut self, vault_path: &str) {
        let path = PathBuf::from(vault_path);
        let keyfile_ref = self.unlock_keyfile_path.as_deref();
        match vault::unlock_vault(&path, &self.unlock_password, keyfile_ref) {
            Ok(key) => {
                self.unlocked_vaults.insert(path, key);
                self.view_mode = ViewMode::Unlocked;
                self.unlock_password.clear();
                self.status_message = Some(("Vault unlocked successfully!".to_string(), false));
                self.refresh_vault_files();
            }
            Err(e) => {
                self.status_message = Some((format!("Unlock failed: {e}"), true));
            }
        }
    }

    fn render_unlocked_view(&mut self, ui: &mut egui::Ui) {
        let (vault_name, vault_path) = match self
            .selected_vault_idx
            .and_then(|i| self.saved_vaults.get(i))
        {
            Some(v) => (v.name.clone(), PathBuf::from(&v.path)),
            None => return,
        };

        // Header bar with Lock button & Mount button
        ui.horizontal(|ui| {
            ui.label(RichText::new(&vault_name).strong().size(20.0));
            ui.label(
                RichText::new("🔓 Unlocked")
                    .color(Color32::from_rgb(80, 200, 120))
                    .strong(),
            );

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add(Button::new("🔒 Lock Vault").accent(Accent::Red))
                    .clicked()
                {
                    self.lock_active_vault();
                }

                if ui.button("🔑 Password").clicked() {
                    self.change_pw_modal_open = true;
                    self.change_pw_old.clear();
                    self.change_pw_new.clear();
                    self.change_pw_confirm.clear();
                }

                if ui.button("🛡️ Recovery Key").clicked()
                    && let Some(key) = self.unlocked_vaults.get(&vault_path)
                {
                    self.displayed_recovery_key = vault::export_recovery_key(key);
                    self.show_recovery_key_modal_open = true;
                }

                #[cfg(unix)]
                {
                    if let Some((_session, mount_path)) = self.active_mounts.get(&vault_path) {
                        if ui
                            .button(format!("⏏ Unmount FUSE ({})", mount_path.display()))
                            .clicked()
                        {
                            self.active_mounts.remove(&vault_path);
                            self.status_message =
                                Some(("FUSE drive unmounted.".to_string(), false));
                        }
                    } else if ui.button("📂 Mount as FUSE Drive").clicked() {
                        let mount_dir = std::env::temp_dir()
                            .join(format!("cryptomator_{}", vault_name.replace(' ', "_")));
                        if let Err(e) = std::fs::create_dir_all(&mount_dir) {
                            self.status_message =
                                Some((format!("Failed to create mount directory: {e}"), true));
                        } else if let Some(key) = self.unlocked_vaults.get(&vault_path) {
                            let fs_impl =
                                crate::fs_fuse::CryptomatorFS::new(vault_path.clone(), *key);
                            let mount_opts = vec![fuser::MountOption::CUSTOM(
                                "fsname=cryptomator-rs".to_string(),
                            )];
                            match fuser::spawn_mount2(fs_impl, &mount_dir, &mount_opts) {
                                Ok(session) => {
                                    self.active_mounts
                                        .insert(vault_path.clone(), (session, mount_dir.clone()));
                                    self.status_message = Some((
                                        format!("Mounted at {}", mount_dir.display()),
                                        false,
                                    ));
                                }
                                Err(e) => {
                                    self.status_message =
                                        Some((format!("FUSE mount error: {e}"), true));
                                }
                            }
                        }
                    }
                }
            });
        });

        ui.add_space(8.0);
        ui.label(
            RichText::new(vault_path.to_string_lossy())
                .size(11.0)
                .color(Color32::from_rgb(140, 150, 175)),
        );
        ui.add_space(12.0);

        // Tab Selector: Files vs Stats
        ui.horizontal(|ui| {
            if ui
                .selectable_label(self.unlocked_tab == UnlockedTab::Files, "📁 Files Explorer")
                .clicked()
            {
                self.unlocked_tab = UnlockedTab::Files;
            }
            if ui
                .selectable_label(
                    self.unlocked_tab == UnlockedTab::Stats,
                    "📊 Activity & Vault Info",
                )
                .clicked()
            {
                self.unlocked_tab = UnlockedTab::Stats;
            }
        });

        ui.separator();
        ui.add_space(8.0);

        match self.unlocked_tab {
            UnlockedTab::Files => {
                self.render_files_tab(ui, &vault_path);
            }
            UnlockedTab::Stats => {
                self.render_stats_tab(ui, &vault_path);
            }
        }
    }

    fn render_files_tab(&mut self, ui: &mut egui::Ui, vault_path: &Path) {
        let master_key = match self.unlocked_vaults.get(vault_path) {
            Some(k) => *k,
            None => return,
        };

        // Action Toolbar
        ui.horizontal(|ui| {
            if ui.button("🔄 Refresh").clicked() {
                self.refresh_vault_files();
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.file_search_query)
                    .hint_text("🔍 Search...")
                    .desired_width(120.0),
            );
            if ui
                .add(Button::new("📥 Import File").accent(Accent::Blue))
                .clicked()
                && let Some(file_path) = rfd::FileDialog::new().pick_file()
            {
                match std::fs::read(&file_path) {
                    Ok(content) => {
                        let file_name = file_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "imported_file".to_string());
                        let size_kb = content.len() as f32 / 1024.0;
                        match vault::write_file(vault_path, &master_key, &file_name, &content) {
                            Ok(_) => {
                                self.active_write_speed = size_kb.max(50.0);
                                self.refresh_vault_files();
                                self.status_message =
                                    Some((format!("Imported '{file_name}' successfully."), false));
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Error writing file: {e}"), true));
                            }
                        }
                    }
                    Err(e) => {
                        self.status_message =
                            Some((format!("Error reading input file: {e}"), true));
                    }
                }
            }
            if ui.button("📂 Import Folder").clicked()
                && let Some(folder_path) = rfd::FileDialog::new().pick_folder()
            {
                match vault::import_dir(vault_path, &master_key, &folder_path, None) {
                    Ok(count) => {
                        self.refresh_vault_files();
                        self.status_message = Some((
                            format!("Imported {count} file(s) from {}", folder_path.display()),
                            false,
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some((format!("Error importing folder: {e}"), true));
                    }
                }
            }
            if ui.button("📦 Export Vault").clicked()
                && let Some(dest_dir) = rfd::FileDialog::new().pick_folder()
            {
                match vault::export_vault(vault_path, &master_key, &dest_dir) {
                    Ok(count) => {
                        self.status_message = Some((
                            format!(
                                "Exported {count} decrypted file(s) to {}",
                                dest_dir.display()
                            ),
                            false,
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some((format!("Error exporting vault: {e}"), true));
                    }
                }
            }
            if ui.button("📁 New Folder").clicked() {
                self.new_folder_modal_open = !self.new_folder_modal_open;
            }
            if ui.button("➕ New Text File").clicked() {
                self.new_file_modal_open = !self.new_file_modal_open;
            }
        });

        ui.add_space(8.0);

        // New Folder Inline Card
        if self.new_folder_modal_open {
            Card::new().heading("Create New Folder").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Folder path:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_folder_name)
                            .hint_text("documents/subfolder"),
                    );
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(Button::new("Create Folder").accent(Accent::Green))
                        .clicked()
                    {
                        let name = self.new_folder_name.trim().to_string();
                        if name.is_empty() {
                            self.status_message =
                                Some(("Folder path cannot be empty.".to_string(), true));
                        } else {
                            match vault::create_dir(vault_path, &master_key, &name) {
                                Ok(_) => {
                                    self.new_folder_modal_open = false;
                                    self.new_folder_name.clear();
                                    self.refresh_vault_files();
                                    self.status_message =
                                        Some((format!("Folder '{name}' created."), false));
                                }
                                Err(e) => {
                                    self.status_message =
                                        Some((format!("Error creating folder: {e}"), true));
                                }
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.new_folder_modal_open = false;
                    }
                });
            });
            ui.add_space(10.0);
        }

        ui.add_space(8.0);

        // New Text File Inline Card
        if self.new_file_modal_open {
            Card::new().heading("Create New Text File").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filename:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_file_name).hint_text("notes.txt"),
                    );
                });
                ui.add_space(6.0);
                ui.label("Content:");
                ui.add(egui::TextEdit::multiline(&mut self.new_file_content).desired_rows(4));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(Button::new("Save File").accent(Accent::Green))
                        .clicked()
                    {
                        if self.new_file_name.trim().is_empty() {
                            self.status_message =
                                Some(("Filename cannot be empty.".to_string(), true));
                        } else {
                            let bytes = self.new_file_content.as_bytes();
                            let size_kb = bytes.len() as f32 / 1024.0;
                            match vault::write_file(
                                vault_path,
                                &master_key,
                                self.new_file_name.trim(),
                                bytes,
                            ) {
                                Ok(_) => {
                                    self.active_write_speed = size_kb.max(10.0);
                                    self.new_file_modal_open = false;
                                    self.new_file_name.clear();
                                    self.new_file_content.clear();
                                    self.refresh_vault_files();
                                    self.status_message =
                                        Some(("File saved to vault.".to_string(), false));
                                }
                                Err(e) => {
                                    self.status_message =
                                        Some((format!("Error saving file: {e}"), true));
                                }
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.new_file_modal_open = false;
                    }
                });
            });
            ui.add_space(10.0);
        }

        if self.rename_modal_open {
            Card::new().heading("Rename File").show(ui, |ui| {
                ui.label(format!("Original path: {}", self.rename_target));
                ui.label("New name / relative path:");
                ui.text_edit_singleline(&mut self.rename_new_name);
                ui.horizontal(|ui| {
                    if ui.button("💾 Save").clicked() {
                        let new_name = self.rename_new_name.trim().to_string();
                        if !new_name.is_empty() && new_name != self.rename_target {
                            match vault::rename_file(
                                vault_path,
                                &master_key,
                                &self.rename_target,
                                &new_name,
                            ) {
                                Ok(_) => {
                                    let old_name = self.rename_target.clone();
                                    self.rename_modal_open = false;
                                    self.refresh_vault_files();
                                    self.status_message = Some((
                                        format!("Renamed '{old_name}' to '{new_name}'."),
                                        false,
                                    ));
                                }
                                Err(e) => {
                                    self.status_message =
                                        Some((format!("Error renaming file: {e}"), true));
                                }
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.rename_modal_open = false;
                    }
                });
            });
            ui.add_space(10.0);
        }

        if self.change_pw_modal_open {
            Card::new().heading("Change Vault Password").show(ui, |ui| {
                ui.label("Current Password:");
                ui.add(egui::TextEdit::singleline(&mut self.change_pw_old).password(true));

                ui.horizontal(|ui| {
                    let old_kf_str = match &self.change_pw_old_keyfile {
                        Some(p) => p.to_string_lossy().to_string(),
                        None => "No current keyfile (optional)".to_string(),
                    };
                    ui.label(RichText::new(old_kf_str).size(12.0).italics());
                    if ui.button("Select Current Keyfile...").clicked()
                        && let Some(file) = rfd::FileDialog::new().pick_file()
                    {
                        self.change_pw_old_keyfile = Some(file);
                    }
                    if self.change_pw_old_keyfile.is_some() && ui.button("Clear").clicked() {
                        self.change_pw_old_keyfile = None;
                    }
                });

                ui.label("New Password:");
                ui.add(egui::TextEdit::singleline(&mut self.change_pw_new).password(true));
                ui.label("Confirm New Password:");
                ui.add(egui::TextEdit::singleline(&mut self.change_pw_confirm).password(true));

                ui.horizontal(|ui| {
                    let new_kf_str = match &self.change_pw_new_keyfile {
                        Some(p) => p.to_string_lossy().to_string(),
                        None => "No new keyfile (optional)".to_string(),
                    };
                    ui.label(RichText::new(new_kf_str).size(12.0).italics());
                    if ui.button("Select New Keyfile...").clicked()
                        && let Some(file) = rfd::FileDialog::new().pick_file()
                    {
                        self.change_pw_new_keyfile = Some(file);
                    }
                    if self.change_pw_new_keyfile.is_some() && ui.button("Clear").clicked() {
                        self.change_pw_new_keyfile = None;
                    }
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(Button::new("Update Password").accent(Accent::Green))
                        .clicked()
                    {
                        if self.change_pw_old.is_empty() || self.change_pw_new.is_empty() {
                            self.status_message =
                                Some(("Passwords cannot be empty.".to_string(), true));
                        } else if self.change_pw_new != self.change_pw_confirm {
                            self.status_message =
                                Some(("New passwords do not match.".to_string(), true));
                        } else {
                            match vault::change_vault_password(
                                vault_path,
                                &self.change_pw_old,
                                self.change_pw_old_keyfile.as_deref(),
                                &self.change_pw_new,
                                self.change_pw_new_keyfile.as_deref(),
                            ) {
                                Ok(_) => {
                                    self.change_pw_modal_open = false;
                                    self.change_pw_old.clear();
                                    self.change_pw_new.clear();
                                    self.change_pw_confirm.clear();
                                    self.change_pw_old_keyfile = None;
                                    self.change_pw_new_keyfile = None;
                                    self.status_message =
                                        Some(("Password updated successfully!".to_string(), false));
                                }
                                Err(e) => {
                                    self.status_message =
                                        Some((format!("Password change failed: {e}"), true));
                                }
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.change_pw_modal_open = false;
                        self.change_pw_old_keyfile = None;
                        self.change_pw_new_keyfile = None;
                    }
                });
            });
            ui.add_space(10.0);
        }

        if self.show_recovery_key_modal_open {
            Card::new().heading("Vault Recovery Key").show(ui, |ui| {
                ui.label("Keep this key safe! It allows unlocking your vault without a password.");
                ui.add_space(6.0);
                ui.label(
                    RichText::new(&self.displayed_recovery_key)
                        .monospace()
                        .size(13.0)
                        .color(Color32::from_rgb(100, 200, 255)),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("📋 Copy to Clipboard").clicked() {
                        ui.ctx().copy_text(self.displayed_recovery_key.clone());
                        self.status_message =
                            Some(("Recovery key copied to clipboard!".to_string(), false));
                    }
                    if ui.button("Close").clicked() {
                        self.show_recovery_key_modal_open = false;
                    }
                });
            });
            ui.add_space(10.0);
        }

        if self.preview_modal_open {
            Card::new()
                .heading(format!("File Viewer: {}", self.preview_file_name))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(250.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.preview_file_text)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::TextStyle::Monospace),
                            );
                        });
                    ui.add_space(6.0);
                    if ui.button("Close Viewer").clicked() {
                        self.preview_modal_open = false;
                    }
                });
            ui.add_space(10.0);
        }

        // Files List
        egui::ScrollArea::vertical().show(ui, |ui| {
            let query = self.file_search_query.trim().to_lowercase();
            let files_to_display: Vec<&vault::FileItem> = self
                .vault_files
                .iter()
                .filter(|f| !f.path.ends_with("/.keep"))
                .filter(|f| query.is_empty() || f.path.to_lowercase().contains(&query))
                .collect();

            if self.vault_files.is_empty() {
                ui.add_space(20.0);
                ui.label(RichText::new("Vault is empty. Click 'Import File' or 'New Text File' above to add documents.").color(Color32::GRAY).italics());
            } else if files_to_display.is_empty() && !query.is_empty() {
                ui.add_space(20.0);
                ui.label(RichText::new(format!("No files matching '{query}'.")).color(Color32::GRAY).italics());
            } else {
                let mut file_to_export = None;
                let mut file_to_delete = None;
                let mut file_to_rename = None;
                let mut file_to_preview = None;

                for file in files_to_display {
                    ui.horizontal(|ui| {
                        let icon = file_icon(&file.path);
                        ui.label(RichText::new(icon).size(16.0));
                        ui.label(RichText::new(&file.path).strong());
                        let size_and_date = if let Some(m) = file.modified {
                            format!(
                                "({} • {})",
                                format_bytes_gui(file.size),
                                vault::format_timestamp(m)
                            )
                        } else {
                            format!("({})", format_bytes_gui(file.size))
                        };
                        ui.label(
                            RichText::new(size_and_date)
                                .color(Color32::from_rgb(140, 150, 170))
                                .size(12.0),
                        );

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("🗑 Delete").clicked() {
                                file_to_delete = Some(file.path.clone());
                            }
                            if ui.button("✏ Rename").clicked() {
                                file_to_rename = Some(file.path.clone());
                            }
                            if ui.button("💾 Export").clicked() {
                                file_to_export = Some(file.path.clone());
                            }
                            if ui.button("👁 View").clicked() {
                                file_to_preview = Some(file.path.clone());
                            }
                        });
                    });
                    ui.separator();
                }

                if let Some(target) = file_to_preview {
                    match vault::read_file(vault_path, &master_key, &target) {
                        Ok(data) => {
                            let text = String::from_utf8(data).unwrap_or_else(|_| {
                                "[Binary or non-UTF-8 file content]".to_string()
                            });
                            self.preview_modal_open = true;
                            self.preview_file_name = target;
                            self.preview_file_text = text;
                        }
                        Err(e) => {
                            self.status_message =
                                Some((format!("Error reading file for preview: {e}"), true));
                        }
                    }
                }

                if let Some(target) = file_to_rename {
                    self.rename_modal_open = true;
                    self.rename_target = target.clone();
                    self.rename_new_name = target;
                }

                if let Some(target) = file_to_export {
                    match vault::read_file(vault_path, &master_key, &target) {
                        Ok(data) => {
                            let size_kb = data.len() as f32 / 1024.0;
                            let filename = Path::new(&target)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "export.dat".to_string());
                            if let Some(dest) = rfd::FileDialog::new().set_file_name(&filename).save_file() {
                                if let Err(e) = std::fs::write(&dest, data) {
                                    self.status_message = Some((format!("Export failed: {e}"), true));
                                } else {
                                    self.active_read_speed = size_kb.max(50.0);
                                    self.status_message = Some((format!("Exported '{target}' to {}", dest.display()), false));
                                }
                            }
                        }
                        Err(e) => {
                            self.status_message = Some((format!("Error decrypting file: {e}"), true));
                        }
                    }
                }

                if let Some(target) = file_to_delete {
                    match vault::delete_file(vault_path, &master_key, &target) {
                        Ok(_) => {
                            self.refresh_vault_files();
                            self.status_message = Some((format!("Deleted '{target}' from vault."), false));
                        }
                        Err(e) => {
                            self.status_message = Some((format!("Error deleting file: {e}"), true));
                        }
                    }
                }
            }
        });
    }

    fn render_stats_tab(&mut self, ui: &mut egui::Ui, vault_path: &Path) {
        let total_size: u64 = self.vault_files.iter().map(|f| f.size).sum();
        let non_keep_count = self
            .vault_files
            .iter()
            .filter(|f| !f.path.ends_with("/.keep"))
            .count();

        Card::new()
            .heading("Cryptographic Architecture")
            .show(ui, |ui| {
                ui.label("• Cipher: AES-256-GCM (Authenticated Encryption)");
                ui.label("• Key Derivation: Scrypt (N=16384, r=8, p=1)");
                ui.label(
                    "• Path & File Obfuscation: Deterministic SHA-256(master_key || relative_path)",
                );
                ui.label(format!("• Decrypted Files in Vault: {non_keep_count}"));
                ui.label(format!(
                    "• Total Storage Used: {}",
                    format_bytes_gui(total_size)
                ));
                #[cfg(unix)]
                {
                    let is_fuse = self
                        .selected_vault_idx
                        .and_then(|i| self.saved_vaults.get(i))
                        .map(|v| self.active_mounts.contains_key(Path::new(&v.path)))
                        .unwrap_or(false);
                    let fuse_str = if is_fuse {
                        "Active (Mounted)"
                    } else {
                        "Inactive"
                    };
                    ui.label(format!("• FUSE Mount Status: {fuse_str}"));
                }
            });

        ui.add_space(12.0);
        Card::new()
            .heading("Cryptographic Integrity Verification")
            .show(ui, |ui| {
                ui.label(
                    "Verify all encrypted files against their stored AES-256-GCM authentication tags.",
                );
                ui.add_space(6.0);
                if let Some(master_key) = self.unlocked_vaults.get(vault_path)
                    && ui.button("🛡️ Verify Vault Integrity Now").clicked()
                {
                    match vault::verify_vault_integrity(vault_path, master_key) {
                            Ok(report) => {
                                if report.is_healthy {
                                    self.status_message = Some((
                                        format!(
                                            "Integrity check passed! All {} files are valid and healthy.",
                                            report.total_checked
                                        ),
                                        false,
                                    ));
                                } else {
                                    self.status_message = Some((
                                        format!(
                                            "Integrity issues found: {} corrupted, {} orphaned files.",
                                            report.corrupted_files.len(),
                                            report.orphaned_files.len()
                                        ),
                                        true,
                                    ));
                                }
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Verification failed: {e}"), true));
                            }
                        }
                }
            });

        ui.add_space(12.0);
        Card::new()
            .heading("Masterkey Backup & Disaster Recovery")
            .show(ui, |ui| {
                ui.label(
                    "Create automated backups of masterkey.json or restore from a backup file.",
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("💾 Backup Masterkey Now").clicked() {
                        match vault::create_masterkey_backup(vault_path, None) {
                            Ok(dest) => {
                                self.status_message = Some((
                                    format!("Masterkey backup saved at {}", dest.display()),
                                    false,
                                ));
                            }
                            Err(e) => {
                                self.status_message =
                                    Some((format!("Masterkey backup failed: {e}"), true));
                            }
                        }
                    }
                    if ui.button("🔄 Restore from Backup...").clicked() {
                        let filter = rfd::FileDialog::new()
                            .set_directory(vault_path)
                            .add_filter("Backup Key", &["bkup", "json"]);
                        if let Some(bkup_file) = filter.pick_file() {
                            match vault::restore_masterkey_backup(vault_path, Some(&bkup_file)) {
                                Ok(_) => {
                                    self.status_message = Some((
                                        format!(
                                            "Masterkey successfully restored from {}",
                                            bkup_file.display()
                                        ),
                                        false,
                                    ));
                                }
                                Err(e) => {
                                    self.status_message =
                                        Some((format!("Masterkey restore failed: {e}"), true));
                                }
                            }
                        }
                    }
                });
            });

        ui.add_space(12.0);
        Card::new()
            .heading("Live Activity & Throughput")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Read: {:.1} KB/s", self.active_read_speed))
                            .color(Color32::from_rgb(100, 180, 255))
                            .strong(),
                    );
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(format!("Write: {:.1} KB/s", self.active_write_speed))
                            .color(Color32::from_rgb(255, 160, 100))
                            .strong(),
                    );
                });

                ui.add_space(12.0);
                ui.label(
                    RichText::new("Throughput Wave Visualizer:")
                        .size(12.0)
                        .color(Color32::from_rgb(140, 150, 175)),
                );

                // Custom painted sparkline/wave visualizer
                let (rect, _response) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 70.0), Sense::hover());
                let painter = ui.painter_at(rect);

                painter.rect_filled(rect, 4.0, Color32::from_rgb(20, 24, 32));

                let points: Vec<f32> = self.throughput_history.iter().copied().collect();
                if points.len() > 1 {
                    let dx = rect.width() / (points.len() - 1) as f32;
                    let max_h = rect.height() - 10.0;

                    for i in 0..points.len() - 1 {
                        let p1_x = rect.left() + i as f32 * dx;
                        let p1_y = rect.bottom() - (points[i] / 100.0).clamp(0.05, 1.0) * max_h;
                        let p2_x = rect.left() + (i + 1) as f32 * dx;
                        let p2_y = rect.bottom() - (points[i + 1] / 100.0).clamp(0.05, 1.0) * max_h;

                        painter.line_segment(
                            [egui::pos2(p1_x, p1_y), egui::pos2(p2_x, p2_y)],
                            Stroke::new(2.0, Color32::from_rgb(80, 150, 240)),
                        );
                    }
                }
            });
    }
}

fn format_bytes_gui(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn file_icon(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".svg")
    {
        "🖼️"
    } else if lower.ends_with(".pdf") {
        "📕"
    } else if lower.ends_with(".zip")
        || lower.ends_with(".tar")
        || lower.ends_with(".gz")
        || lower.ends_with(".7z")
        || lower.ends_with(".rar")
    {
        "📦"
    } else if lower.ends_with(".mp3")
        || lower.ends_with(".wav")
        || lower.ends_with(".flac")
        || lower.ends_with(".mp4")
        || lower.ends_with(".mkv")
    {
        "🎬"
    } else if lower.ends_with(".rs")
        || lower.ends_with(".py")
        || lower.ends_with(".js")
        || lower.ends_with(".json")
        || lower.ends_with(".toml")
        || lower.ends_with(".html")
        || lower.ends_with(".css")
        || lower.ends_with(".md")
    {
        "💻"
    } else {
        "📄"
    }
}
