mod vault;
mod fs_fuse;
mod cli;
mod tui_app;

use slint::{ModelRc, VecModel, SharedString, Model};
use std::rc::Rc;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use serde::{Serialize, Deserialize};
use clap::Parser;
use std::io::Write;

slint::include_modules!();

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SavedVault {
    name: String,
    path: String,
}

struct ActiveMount {
    _session: fuser::BackgroundSession,
    mount_path: PathBuf,
}

const CONFIG_FILE: &str = "vaults.json";

fn load_saved_vaults() -> Vec<SavedVault> {
    if Path::new(CONFIG_FILE).exists() {
        if let Ok(content) = fs::read_to_string(CONFIG_FILE) {
            if let Ok(vaults) = serde_json::from_str::<Vec<SavedVault>>(&content) {
                return vaults;
            }
        }
    }
    Vec::new()
}

fn save_saved_vaults(vaults: &[SavedVault]) {
    if let Ok(content) = serde_json::to_string_pretty(vaults) {
        let _ = fs::write(CONFIG_FILE, content);
    }
}

fn main() {
    let args = cli::CliArgs::parse();

    if args.tui {
        if let Err(e) = tui_app::run_tui() {
            eprintln!("Error running TUI: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if let Some(command) = args.command {
        match command {
            cli::Commands::Tui => {
                if let Err(e) = tui_app::run_tui() {
                    eprintln!("Error running TUI: {}", e);
                    std::process::exit(1);
                }
                return;
            }
            cli::Commands::List => {
                let vaults = load_saved_vaults();
                if vaults.is_empty() {
                    println!("No registered vaults found.");
                } else {
                    println!("Registered vaults:");
                    for (i, v) in vaults.iter().enumerate() {
                        println!("  [{}] {} ({})", i, v.name, v.path);
                    }
                }
                return;
            }
            cli::Commands::Create { name, path, password } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        print!("Enter password for new vault: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p).unwrap();
                        p.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::create_vault(path_buf, &pwd, None) {
                    Ok(_) => {
                        println!("Vault '{}' created successfully at '{}'!", name, path);
                        let mut vaults = load_saved_vaults();
                        vaults.push(SavedVault { name, path });
                        save_saved_vaults(&vaults);
                    }
                    Err(e) => {
                        eprintln!("Error creating vault: {}", e);
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Mount { path, mountpoint, password } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        print!("Enter password to unlock vault: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p).unwrap();
                        p.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::unlock_vault(path_buf, &pwd, None) {
                    Ok(master_key) => {
                        let mount_dir = Path::new(&mountpoint);
                        if let Err(e) = std::fs::create_dir_all(mount_dir) {
                            eprintln!("Error creating mount directory: {}", e);
                            std::process::exit(1);
                        }
                        
                        println!("Spawning FUSE mount at {:?}...", mount_dir);
                        let fs_impl = crate::fs_fuse::CryptomatorFS::new(path_buf.to_path_buf(), master_key);
                        let mount_options = vec![
                            fuser::MountOption::CUSTOM("fsname=cryptomator-rs".to_string()),
                        ];
                        
                        match fuser::spawn_mount2(fs_impl, mount_dir, &mount_options) {
                            Ok(_session) => {
                                println!("Vault mounted successfully at {:?}.", mount_dir);
                                println!("Press Ctrl+C to unmount.");
                                loop {
                                    std::thread::sleep(std::time::Duration::from_secs(1));
                                }
                            }
                            Err(e) => {
                                eprintln!("Mount failed: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Unlock failed: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
    }

    let ui = AppWindow::new().unwrap();
    
    // Load config
    let saved_vaults = Rc::new(RefCell::new(load_saved_vaults()));
    
    // Keep track of decrypted master keys in memory: path -> master_key
    let unlocked_vaults = Rc::new(RefCell::new(HashMap::<PathBuf, [u8; 32]>::new()));
    
    // Keep track of active mounts: path -> ActiveMount
    let active_mounts = Rc::new(RefCell::new(HashMap::<PathBuf, ActiveMount>::new()));
    
    // Keep track of last interaction time for Auto-Lock
    let last_interaction = Rc::new(RefCell::new(std::time::Instant::now()));
    
    // Keep track of current read/write throughput speeds
    let active_read_speed = Rc::new(RefCell::new(0.0));
    let active_write_speed = Rc::new(RefCell::new(0.0));
    
    // Set up Slint model for Vaults
    let vaults_model = Rc::new(VecModel::<VaultData>::default());
    
    // Populate vaults model initially
    {
        let saved = saved_vaults.borrow();
        for v in saved.iter() {
            vaults_model.push(VaultData {
                name: SharedString::from(&v.name),
                path: SharedString::from(&v.path),
                locked: true,
            });
        }
    }
    ui.set_vaults(ModelRc::from(vaults_model.clone()));
    
    // Setup callbacks
    
    // Callback: Select Vault
    let ui_weak = ui.as_weak();
    let saved_vaults_clone = saved_vaults.clone();
    let unlocked_vaults_clone = unlocked_vaults.clone();
    let vaults_model_clone = vaults_model.clone();
    let last_interaction_clone = last_interaction.clone();
    ui.on_select_vault(move |idx| {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let idx_usize = idx as usize;
        
        let saved = saved_vaults_clone.borrow();
        if idx_usize >= saved.len() {
            return;
        }
        
        let path_str = &saved[idx_usize].path;
        let path = PathBuf::from(path_str);
        
        ui.set_active_vault_index(idx);
        ui.set_status_message(SharedString::new());
        ui.set_password_input(SharedString::new());
        
        let unlocked = unlocked_vaults_clone.borrow();
        let is_unlocked = unlocked.contains_key(&path);
        
        // Update the Slint model locked state
        if let Some(mut v) = vaults_model_clone.row_data(idx_usize) {
            v.locked = !is_unlocked;
            vaults_model_clone.set_row_data(idx_usize, v);
        }
        
        if is_unlocked {
            ui.set_active_view(SharedString::from("unlocked"));
        } else {
            ui.set_active_view(SharedString::from("locked"));
        }
    });
    
    // Callback: Unlock Vault
    let ui_weak = ui.as_weak();
    let saved_vaults_clone = saved_vaults.clone();
    let unlocked_vaults_clone = unlocked_vaults.clone();
    let active_mounts_clone = active_mounts.clone();
    let vaults_model_clone = vaults_model.clone();
    let last_interaction_clone = last_interaction.clone();
    ui.on_unlock_vault(move |password| {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let idx = ui.get_active_vault_index() as usize;
        
        let saved = saved_vaults_clone.borrow();
        if idx >= saved.len() {
            ui.set_status_message(SharedString::from("No vault selected."));
            ui.set_status_is_error(true);
            return;
        }
        
        let path_str = &saved[idx].path;
        let path = Path::new(path_str);
        
        let keyfile_path_str = ui.get_unlock_keyfile_path();
        let keyfile_path = if ui.get_unlock_use_keyfile() && !keyfile_path_str.is_empty() {
            Some(Path::new(keyfile_path_str.as_str()))
        } else {
            None
        };
        
        match vault::unlock_vault(path, password.as_str(), keyfile_path) {
            Ok(master_key) => {
                // Setup and create FUSE mount directory in user home: ~/CryptomatorMounts/<vault_name>
                let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/home/adrichel".to_string());
                let mount_base = Path::new(&home_dir).join("CryptomatorMounts");
                let vault_name = path.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "vault".to_string());
                let mount_dir = mount_base.join(&vault_name);
                
                if let Err(e) = fs::create_dir_all(&mount_dir) {
                    eprintln!("[LOG] Mount Error: Failed to create mount directory {:?}: {}", mount_dir, e);
                    ui.set_status_message(SharedString::from(format!("Failed to create mount folder: {}", e)));
                    ui.set_status_is_error(true);
                    return;
                }
                
                println!("[LOG] Mount: Spawning FUSE mount at {:?}", mount_dir);
                let fs_impl = crate::fs_fuse::CryptomatorFS::new(path.to_path_buf(), master_key);
                let mount_options = vec![
                    fuser::MountOption::CUSTOM("fsname=cryptomator-rs".to_string()),
                ];
                
                match fuser::spawn_mount2(fs_impl, &mount_dir, &mount_options) {
                    Ok(session) => {
                        println!("[LOG] Mount: FUSE mounted successfully at {:?}", mount_dir);
                        unlocked_vaults_clone.borrow_mut().insert(path.to_path_buf(), master_key);
                        active_mounts_clone.borrow_mut().insert(path.to_path_buf(), ActiveMount {
                            _session: session,
                            mount_path: mount_dir.clone(),
                        });
                        
                        // Update vaults list model
                        if let Some(mut v) = vaults_model_clone.row_data(idx) {
                            v.locked = false;
                            vaults_model_clone.set_row_data(idx, v);
                        }
                        
                        ui.set_active_view(SharedString::from("unlocked"));
                        ui.set_status_message(SharedString::from(format!("Vault unlocked! Mounted at {:?}", mount_dir)));
                        ui.set_status_is_error(false);
                        ui.set_password_input(SharedString::new());
                    }
                    Err(e) => {
                        eprintln!("[LOG] Mount Error: {:?}", e);
                        ui.set_status_message(SharedString::from(format!("Mount failed: {}", e)));
                        ui.set_status_is_error(true);
                    }
                }
            }
            Err(e) => {
                ui.set_status_message(SharedString::from(format!("Failed to unlock: {}", e)));
                ui.set_status_is_error(true);
            }
        }
    });
    
    // Callback: Lock Vault
    let ui_weak = ui.as_weak();
    let saved_vaults_clone = saved_vaults.clone();
    let unlocked_vaults_clone = unlocked_vaults.clone();
    let active_mounts_clone = active_mounts.clone();
    let vaults_model_clone = vaults_model.clone();
    let last_interaction_clone = last_interaction.clone();
    ui.on_lock_vault(move || {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let idx = ui.get_active_vault_index() as usize;
        
        let saved = saved_vaults_clone.borrow();
        if idx >= saved.len() {
            return;
        }
        
        let path_str = &saved[idx].path;
        let path = PathBuf::from(path_str);
        
        unlocked_vaults_clone.borrow_mut().remove(&path);
        
        // Dropping the ActiveMount will drop the fuser::BackgroundSession, which automatically unmounts FUSE
        if let Some(mount) = active_mounts_clone.borrow_mut().remove(&path) {
            println!("[LOG] Lock: Unmounting FUSE at {:?}", mount.mount_path);
        }
        
        // Update model
        if let Some(mut v) = vaults_model_clone.row_data(idx) {
            v.locked = true;
            vaults_model_clone.set_row_data(idx, v);
        }
        
        ui.set_active_view(SharedString::from("locked"));
        ui.set_status_message(SharedString::from("Vault locked."));
        ui.set_status_is_error(false);
    });
    
    // Callback: Show Create Wizard
    let ui_weak = ui.as_weak();
    ui.on_show_create_wizard(move || {
        let ui = ui_weak.unwrap();
        ui.set_active_view(SharedString::from("create_wizard"));
        ui.set_new_vault_name(SharedString::new());
        ui.set_new_vault_path(SharedString::new());
        ui.set_new_vault_password(SharedString::new());
        ui.set_status_message(SharedString::new());
    });
    
    // Callback: Cancel Wizard
    let ui_weak = ui.as_weak();
    ui.on_cancel_wizard(move || {
        let ui = ui_weak.unwrap();
        ui.set_active_view(SharedString::from("welcome"));
        ui.set_active_vault_index(-1);
    });
    
    // Callback: Add Existing Vault
    let ui_weak = ui.as_weak();
    let saved_vaults_clone = saved_vaults.clone();
    let vaults_model_clone = vaults_model.clone();
    ui.on_add_existing_vault(move || {
        let ui = ui_weak.unwrap();
        
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select Existing Cryptomator Vault Folder")
            .pick_folder() 
        {
            println!("[LOG] Add Existing: Selected folder path: {:?}", path);
            
            // Verify if masterkey.json exists in this folder
            let mk_path = path.join("masterkey.json");
            if !mk_path.exists() {
                eprintln!("[LOG] Add Existing Error: 'masterkey.json' not found at {:?}", mk_path);
                ui.set_status_message(SharedString::from("Selected directory is not a valid Cryptomator RS vault (masterkey.json missing)"));
                ui.set_status_is_error(true);
                return;
            }
            
            let name = path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Existing Vault".to_string());
                
            let path_str = path.to_string_lossy().into_owned();
            
            // Check if already added
            let mut saved = saved_vaults_clone.borrow_mut();
            if saved.iter().any(|v| v.path == path_str) {
                eprintln!("[LOG] Add Existing Error: Vault already in the list: {}", path_str);
                ui.set_status_message(SharedString::from("Vault is already in the list."));
                ui.set_status_is_error(true);
                return;
            }
            
            saved.push(SavedVault {
                name: name.clone(),
                path: path_str.clone(),
            });
            save_saved_vaults(&saved);
            
            vaults_model_clone.push(VaultData {
                name: SharedString::from(&name),
                path: SharedString::from(&path_str),
                locked: true,
            });
            
            let new_idx = (saved.len() - 1) as i32;
            ui.set_active_vault_index(new_idx);
            ui.set_active_view(SharedString::from("locked"));
            ui.set_status_message(SharedString::from("Vault added successfully."));
            ui.set_status_is_error(false);
            println!("[LOG] Add Existing Success: Vault '{name}' added successfully at {path_str}");
        }
    });
    
    // Callback: Select New Vault Folder (in creation wizard)
    let ui_weak = ui.as_weak();
    ui.on_select_new_vault_folder(move || {
        let ui = ui_weak.unwrap();
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Choose Folder to Create Vault")
            .pick_folder() 
        {
            let path_str = path.to_string_lossy().into_owned();
            println!("[LOG] Create Wizard: Selected base folder path: {}", path_str);
            ui.set_new_vault_path(SharedString::from(path_str));
        }
    });
    
    // Callback: Create New Vault
    let ui_weak = ui.as_weak();
    let saved_vaults_clone = saved_vaults.clone();
    let vaults_model_clone = vaults_model.clone();
    let last_interaction_clone = last_interaction.clone();
    ui.on_create_new_vault(move |name, path_str, password| {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        
        let name_str = name.as_str().trim();
        let path_str_trimmed = path_str.as_str().trim();
        let pwd_str = password.as_str();
        
        println!("[LOG] Create Vault Attempt: Name='{}', Path='{}'", name_str, path_str_trimmed);
        
        if name_str.is_empty() || path_str_trimmed.is_empty() || pwd_str.is_empty() {
            eprintln!("[LOG] Create Vault Error: Empty inputs");
            ui.set_status_message(SharedString::from("All fields are required."));
            ui.set_status_is_error(true);
            return;
        }
        
        let path = Path::new(path_str_trimmed);
        
        let keyfile_path_str = ui.get_new_vault_keyfile_path();
        let keyfile_path = if ui.get_new_vault_use_keyfile() && !keyfile_path_str.is_empty() {
            Some(Path::new(keyfile_path_str.as_str()))
        } else {
            None
        };
        
        // Try creating
        match vault::create_vault(path, pwd_str, keyfile_path) {
            Ok(_) => {
                println!("[LOG] Create Vault Success: Vault created on disk at {:?}", path);
                let mut saved = saved_vaults_clone.borrow_mut();
                saved.push(SavedVault {
                    name: name_str.to_string(),
                    path: path_str_trimmed.to_string(),
                });
                save_saved_vaults(&saved);
                
                vaults_model_clone.push(VaultData {
                    name: SharedString::from(name_str),
                    path: SharedString::from(path_str_trimmed),
                    locked: true,
                });
                
                let new_idx = (saved.len() - 1) as i32;
                ui.set_active_vault_index(new_idx);
                ui.set_active_view(SharedString::from("locked"));
                ui.set_status_message(SharedString::from("Vault created successfully! Enter password to unlock."));
                ui.set_status_is_error(false);
            }
            Err(e) => {
                eprintln!("[LOG] Create Vault Error: {:?}", e);
                ui.set_status_message(SharedString::from(format!("Failed to create vault: {}", e)));
                ui.set_status_is_error(true);
            }
        }
    });
    
    // Helper helper to get active vault details
    let get_active_vault_details = {
        let saved_vaults = saved_vaults.clone();
        let unlocked_vaults = unlocked_vaults.clone();
        move |active_idx: i32| -> Option<(PathBuf, [u8; 32])> {
            let idx = active_idx as usize;
            let saved = saved_vaults.borrow();
            if idx >= saved.len() {
                return None;
            }
            let path = PathBuf::from(&saved[idx].path);
            let unlocked = unlocked_vaults.borrow();
            let key = unlocked.get(&path)?;
            Some((path, *key))
        }
    };
    
    // Callback: Select Keyfile for new vault
    let ui_weak = ui.as_weak();
    let last_interaction_clone = last_interaction.clone();
    ui.on_select_new_vault_keyfile(move || {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select Keyfile")
            .pick_file()
        {
            ui.set_new_vault_keyfile_path(SharedString::from(path.to_string_lossy().into_owned()));
        }
    });

    // Callback: Select Keyfile for unlocking
    let ui_weak = ui.as_weak();
    let last_interaction_clone = last_interaction.clone();
    ui.on_select_unlock_keyfile(move || {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select Keyfile")
            .pick_file()
        {
            ui.set_unlock_keyfile_path(SharedString::from(path.to_string_lossy().into_owned()));
        }
    });

    // Callback: Show Files View / Toggle Files View
    let ui_weak = ui.as_weak();
    let get_active_vault_details_clone = get_active_vault_details.clone();
    let last_interaction_clone = last_interaction.clone();
    ui.on_show_files_view(move || {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let view = ui.get_active_view();
        
        if view == "unlocked" {
            ui.set_active_view(SharedString::from("files"));
            
            // Trigger refresh files
            let idx = ui.get_active_vault_index();
            if let Some((path, key)) = get_active_vault_details_clone(idx) {
                match vault::list_files(&path, &key) {
                    Ok(files) => {
                        let file_model = Rc::new(VecModel::<FileData>::default());
                        for f in files {
                            file_model.push(FileData {
                                name: SharedString::from(&f),
                                path: SharedString::from(&f),
                            });
                        }
                        ui.set_vault_files(ModelRc::from(file_model));
                    }
                    Err(e) => {
                        ui.set_status_message(SharedString::from(format!("Failed to list files: {}", e)));
                        ui.set_status_is_error(true);
                    }
                }
            }
        } else if view == "files" {
            ui.set_active_view(SharedString::from("unlocked"));
            ui.set_status_message(SharedString::new());
        }
    });
    
    // Callback: Refresh Files
    let ui_weak = ui.as_weak();
    let get_active_vault_details_clone = get_active_vault_details.clone();
    let last_interaction_clone = last_interaction.clone();
    ui.on_refresh_files(move || {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let idx = ui.get_active_vault_index();
        if let Some((path, key)) = get_active_vault_details_clone(idx) {
            match vault::list_files(&path, &key) {
                Ok(files) => {
                    let file_model = Rc::new(VecModel::<FileData>::default());
                    for f in files {
                        file_model.push(FileData {
                            name: SharedString::from(&f),
                            path: SharedString::from(&f),
                        });
                    }
                    ui.set_vault_files(ModelRc::from(file_model));
                    ui.set_status_message(SharedString::from("Files refreshed."));
                    ui.set_status_is_error(false);
                }
                Err(e) => {
                    ui.set_status_message(SharedString::from(format!("Refresh failed: {}", e)));
                    ui.set_status_is_error(true);
                }
            }
        }
    });
    
    // Callback: Import File
    let ui_weak = ui.as_weak();
    let get_active_vault_details_clone = get_active_vault_details.clone();
    let last_interaction_clone = last_interaction.clone();
    let active_write_speed_clone = active_write_speed.clone();
    ui.on_import_file(move || {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let idx = ui.get_active_vault_index();
        
        let Some((path, key)) = get_active_vault_details_clone(idx) else {
            return;
        };
        
        if let Some(src_path) = rfd::FileDialog::new()
            .set_title("Choose File to Import")
            .pick_file() 
        {
            let filename = src_path.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "imported_file".to_string());
                
            match fs::read(&src_path) {
                Ok(content) => {
                    match vault::write_file(&path, &key, &filename, &content) {
                        Ok(_) => {
                            *active_write_speed_clone.borrow_mut() += (content.len() as f64) / 102.4;
                            ui.set_status_message(SharedString::from(format!("Successfully encrypted & imported {}", filename)));
                            ui.set_status_is_error(false);
                            
                            // Refresh list
                            if let Ok(files) = vault::list_files(&path, &key) {
                                let file_model = Rc::new(VecModel::<FileData>::default());
                                for f in files {
                                    file_model.push(FileData {
                                        name: SharedString::from(&f),
                                        path: SharedString::from(&f),
                                    });
                                }
                                ui.set_vault_files(ModelRc::from(file_model));
                            }
                        }
                        Err(e) => {
                            ui.set_status_message(SharedString::from(format!("Import failed: {}", e)));
                            ui.set_status_is_error(true);
                        }
                    }
                }
                Err(e) => {
                    ui.set_status_message(SharedString::from(format!("Failed to read source file: {}", e)));
                    ui.set_status_is_error(true);
                }
            }
        }
    });
    
    // Callback: Export File
    let ui_weak = ui.as_weak();
    let get_active_vault_details_clone = get_active_vault_details.clone();
    let last_interaction_clone = last_interaction.clone();
    let active_read_speed_clone = active_read_speed.clone();
    ui.on_export_file(move |filename| {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let idx = ui.get_active_vault_index();
        
        let Some((path, key)) = get_active_vault_details_clone(idx) else {
            return;
        };
        
        if let Some(dest_path) = rfd::FileDialog::new()
            .set_title("Export File As")
            .set_file_name(filename.as_str())
            .save_file()
        {
            match vault::read_file(&path, &key, filename.as_str()) {
                Ok(content) => {
                    let content_len = content.len();
                    match fs::write(&dest_path, content) {
                        Ok(_) => {
                            *active_read_speed_clone.borrow_mut() += (content_len as f64) / 102.4;
                            ui.set_status_message(SharedString::from(format!("Successfully exported & decrypted to {}", dest_path.display())));
                            ui.set_status_is_error(false);
                        }
                        Err(e) => {
                            ui.set_status_message(SharedString::from(format!("Failed to write file: {}", e)));
                            ui.set_status_is_error(true);
                        }
                    }
                }
                Err(e) => {
                    ui.set_status_message(SharedString::from(format!("Failed to read file from vault: {}", e)));
                    ui.set_status_is_error(true);
                }
            }
        }
    });
    
    // Callback: Delete File
    let ui_weak = ui.as_weak();
    let get_active_vault_details_clone = get_active_vault_details.clone();
    let last_interaction_clone = last_interaction.clone();
    ui.on_delete_file(move |filename| {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let idx = ui.get_active_vault_index();
        
        let Some((path, key)) = get_active_vault_details_clone(idx) else {
            return;
        };
        
        match vault::delete_file(&path, &key, filename.as_str()) {
            Ok(_) => {
                ui.set_status_message(SharedString::from(format!("Deleted file: {}", filename)));
                ui.set_status_is_error(false);
                
                // Refresh list
                if let Ok(files) = vault::list_files(&path, &key) {
                    let file_model = Rc::new(VecModel::<FileData>::default());
                    for f in files {
                        file_model.push(FileData {
                            name: SharedString::from(&f),
                            path: SharedString::from(&f),
                        });
                    }
                    ui.set_vault_files(ModelRc::from(file_model));
                }
            }
            Err(e) => {
                ui.set_status_message(SharedString::from(format!("Failed to delete: {}", e)));
                ui.set_status_is_error(true);
            }
        }
    });
    
    // Callback: Create Text File
    let ui_weak = ui.as_weak();
    let get_active_vault_details_clone = get_active_vault_details.clone();
    let last_interaction_clone = last_interaction.clone();
    let active_write_speed_clone = active_write_speed.clone();
    ui.on_create_text_file(move |filename, content| {
        *last_interaction_clone.borrow_mut() = std::time::Instant::now();
        let ui = ui_weak.unwrap();
        let idx = ui.get_active_vault_index();
        
        let Some((path, key)) = get_active_vault_details_clone(idx) else {
            return;
        };
        
        let name_str = filename.as_str().trim();
        if name_str.is_empty() {
            ui.set_status_message(SharedString::from("Filename cannot be empty."));
            ui.set_status_is_error(true);
            return;
        }
        
        match vault::write_file(&path, &key, name_str, content.as_bytes()) {
            Ok(_) => {
                *active_write_speed_clone.borrow_mut() += (content.len() as f64) / 102.4;
                ui.set_status_message(SharedString::from(format!("Created file: {}", name_str)));
                ui.set_status_is_error(false);
                ui.set_new_file_name(SharedString::new());
                ui.set_new_file_content(SharedString::new());
                
                // Refresh list
                if let Ok(files) = vault::list_files(&path, &key) {
                    let file_model = Rc::new(VecModel::<FileData>::default());
                    for f in files {
                        file_model.push(FileData {
                            name: SharedString::from(&f),
                            path: SharedString::from(&f),
                        });
                    }
                    ui.set_vault_files(ModelRc::from(file_model));
                }
            }
            Err(e) => {
                ui.set_status_message(SharedString::from(format!("Failed to create file: {}", e)));
                ui.set_status_is_error(true);
            }
        }
    });
    
    // --- TIMERS ---

    // 1. Auto-Lock Timer (runs every second)
    let auto_lock_timer = slint::Timer::default();
    let ui_weak = ui.as_weak();
    let last_interaction_clone = last_interaction.clone();
    let unlocked_vaults_clone = unlocked_vaults.clone();
    let active_mounts_clone = active_mounts.clone();
    let vaults_model_clone = vaults_model.clone();
    auto_lock_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(1000), move || {
        let ui = ui_weak.unwrap();
        
        if ui.get_auto_lock_enabled() {
            let timeout_str = ui.get_auto_lock_timeout_minutes();
            if let Ok(mins) = timeout_str.as_str().trim().parse::<u64>() {
                let elapsed = last_interaction_clone.borrow().elapsed();
                if elapsed >= std::time::Duration::from_secs(mins * 60) {
                    let idx = ui.get_active_vault_index();
                    if idx >= 0 {
                        let saved = load_saved_vaults();
                        if (idx as usize) < saved.len() {
                            let path = PathBuf::from(&saved[idx as usize].path);
                            
                            // Perform FUSE unmount if mounted
                            if let Some(mount) = active_mounts_clone.borrow_mut().remove(&path) {
                                println!("[LOG] Auto-Lock: Unmounting {:?}", mount.mount_path);
                                // The BackgroundSession is dropped here, which triggers auto-unmount
                            }
                            
                            unlocked_vaults_clone.borrow_mut().remove(&path);
                            
                            // Update UI model
                            if let Some(mut v) = vaults_model_clone.row_data(idx as usize) {
                                v.locked = true;
                                vaults_model_clone.set_row_data(idx as usize, v);
                            }
                            
                            ui.set_active_view(SharedString::from("locked"));
                            ui.set_status_message(SharedString::from("Vault auto-locked due to inactivity."));
                            ui.set_status_is_error(false);
                            
                            // Reset interaction to avoid constant messages
                            *last_interaction_clone.borrow_mut() = std::time::Instant::now();
                        }
                    }
                }
            }
        }
    });

    // 2. Real-time Throughput Graph Timer (runs every 500ms)
    let graph_timer = slint::Timer::default();
    let ui_weak = ui.as_weak();
    let active_read_speed_clone = active_read_speed.clone();
    let active_write_speed_clone = active_write_speed.clone();
    graph_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(500), move || {
        let ui = ui_weak.unwrap();
        
        let view = ui.get_active_view();
        if view != "unlocked" && view != "files" {
            return;
        }
        
        // Get history
        let history_model = ui.get_throughput_history();
        let mut history: Vec<i32> = history_model.iter().collect();
        if !history.is_empty() {
            history.remove(0);
        }
        
        // Read active speeds and decay them
        let r_speed = {
            let mut r = *active_read_speed_clone.borrow();
            if r > 0.0 {
                r *= 0.6;
                if r < 0.1 { r = 0.0; }
                *active_read_speed_clone.borrow_mut() = r;
            }
            r
        };
        
        let w_speed = {
            let mut w = *active_write_speed_clone.borrow();
            if w > 0.0 {
                w *= 0.6;
                if w < 0.1 { w = 0.0; }
                *active_write_speed_clone.borrow_mut() = w;
            }
            w
        };
        
        // Determine graph point
        let next_val = if r_speed > 0.1 || w_speed > 0.1 {
            let sum = r_speed + w_speed;
            ((sum / 20.0).min(90.0).max(35.0)) as i32
        } else {
            // Idle wave simulation
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as f64;
            let val = (now / 700.0).sin() * 4.0 + 8.0;
            val as i32
        };
        history.push(next_val);
        
        let model = Rc::new(VecModel::from(history));
        ui.set_throughput_history(ModelRc::from(model));
        
        // Format strings
        let r_str = if r_speed >= 1024.0 {
            format!("{:.1} MB/s", r_speed / 1024.0)
        } else {
            format!("{:.1} KB/s", r_speed)
        };
        
        let w_str = if w_speed >= 1024.0 {
            format!("{:.1} MB/s", w_speed / 1024.0)
        } else {
            format!("{:.1} KB/s", w_speed)
        };
        
        ui.set_read_throughput(SharedString::from(r_str));
        ui.set_write_throughput(SharedString::from(w_str));
    });

    ui.run().unwrap();
}
