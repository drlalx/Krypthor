mod cli;
#[cfg(feature = "egui-ui")]
mod egui_ui;
#[cfg(unix)]
mod fs_fuse;
#[cfg(feature = "tui")]
mod tui_app;
mod vault;
#[cfg(feature = "web")]
pub mod web_server;

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SavedVault {
    pub name: String,
    pub path: String,
}

pub const CONFIG_FILE: &str = "vaults.json";

pub fn load_saved_vaults() -> Vec<SavedVault> {
    if Path::new(CONFIG_FILE).exists()
        && let Ok(content) = fs::read_to_string(CONFIG_FILE)
        && let Ok(vaults) = serde_json::from_str::<Vec<SavedVault>>(&content)
    {
        return vaults;
    }
    Vec::new()
}

pub fn save_saved_vaults(vaults: &[SavedVault]) {
    if let Ok(content) = serde_json::to_string_pretty(vaults) {
        let _ = fs::write(CONFIG_FILE, content);
    }
}

fn main() {
    let args = cli::CliArgs::parse();

    #[cfg(feature = "tui")]
    if args.tui {
        if let Err(e) = tui_app::run_tui() {
            eprintln!("Error running TUI: {e}");
            std::process::exit(1);
        }
        return;
    }

    #[cfg(feature = "web")]
    if args.serve {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to create tokio runtime: {e}");
                std::process::exit(1);
            }
        };
        if let Err(e) = rt.block_on(web_server::run_server("127.0.0.1", 8080)) {
            eprintln!("Web server error: {e}");
            std::process::exit(1);
        }
        return;
    }

    if let Some(command) = args.command {
        match command {
            cli::Commands::Tui => {
                #[cfg(feature = "tui")]
                {
                    if let Err(e) = tui_app::run_tui() {
                        eprintln!("Error running TUI: {e}");
                        std::process::exit(1);
                    }
                    return;
                }
                #[cfg(not(feature = "tui"))]
                {
                    eprintln!("TUI feature is disabled in this build.");
                    std::process::exit(1);
                }
            }
            cli::Commands::Serve { host, port } => {
                #[cfg(feature = "web")]
                {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => {
                            eprintln!("Failed to create tokio runtime: {e}");
                            std::process::exit(1);
                        }
                    };
                    if let Err(e) = rt.block_on(web_server::run_server(&host, port)) {
                        eprintln!("Web server error: {e}");
                        std::process::exit(1);
                    }
                    return;
                }
                #[cfg(not(feature = "web"))]
                {
                    eprintln!("Web feature is disabled in this build.");
                    std::process::exit(1);
                }
            }
            cli::Commands::List => {
                let vaults = load_saved_vaults();
                if vaults.is_empty() {
                    println!("No registered vaults found.");
                } else {
                    println!("Registered vaults:");
                    for (i, v) in vaults.iter().enumerate() {
                        println!("  [{i}] {} ({})", v.name, v.path);
                    }
                }
                return;
            }
            cli::Commands::Create {
                name,
                path,
                password,
            } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        print!("Enter password for new vault: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        let _ = std::io::stdin().read_line(&mut p);
                        p.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::create_vault(path_buf, &pwd, None) {
                    Ok(_) => {
                        println!("Vault '{name}' created successfully at '{path}'!");
                        let mut vaults = load_saved_vaults();
                        vaults.push(SavedVault { name, path });
                        save_saved_vaults(&vaults);
                    }
                    Err(e) => {
                        eprintln!("Error creating vault: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Mount {
                path,
                mountpoint,
                password,
            } => {
                #[cfg(unix)]
                {
                    let pwd = match password {
                        Some(p) => p,
                        None => {
                            print!("Enter password to unlock vault: ");
                            let _ = std::io::stdout().flush();
                            let mut p = String::new();
                            let _ = std::io::stdin().read_line(&mut p);
                            p.trim().to_string()
                        }
                    };
                    let path_buf = Path::new(&path);
                    match vault::unlock_vault(path_buf, &pwd, None) {
                        Ok(master_key) => {
                            let mount_dir = Path::new(&mountpoint);
                            if let Err(e) = std::fs::create_dir_all(mount_dir) {
                                eprintln!("Error creating mount directory: {e}");
                                std::process::exit(1);
                            }

                            println!("Spawning FUSE mount at {mount_dir:?}...");
                            let fs_impl = crate::fs_fuse::CryptomatorFS::new(
                                path_buf.to_path_buf(),
                                master_key,
                            );
                            let mount_options = vec![fuser::MountOption::CUSTOM(
                                "fsname=cryptomator-rs".to_string(),
                            )];

                            match fuser::spawn_mount2(fs_impl, mount_dir, &mount_options) {
                                Ok(_session) => {
                                    println!("Vault mounted successfully at {mount_dir:?}.");
                                    println!("Press Ctrl+C to unmount.");
                                    loop {
                                        std::thread::sleep(std::time::Duration::from_secs(1));
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Mount failed: {e}");
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Unlock failed: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = (path, mountpoint, password);
                    eprintln!("FUSE mount is only supported on Unix/Linux platforms.");
                    std::process::exit(1);
                }
            }
            cli::Commands::ChangePassword {
                path,
                old_password,
                new_password,
            } => {
                let old_pwd = match old_password {
                    Some(p) => p,
                    None => {
                        print!("Enter current password: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        let _ = std::io::stdin().read_line(&mut p);
                        p.trim().to_string()
                    }
                };
                let new_pwd = match new_password {
                    Some(p) => p,
                    None => {
                        print!("Enter new password: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        let _ = std::io::stdin().read_line(&mut p);
                        p.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::change_vault_password(path_buf, &old_pwd, None, &new_pwd, None) {
                    Ok(_) => {
                        println!("Vault password changed successfully for '{path}'!");
                    }
                    Err(e) => {
                        eprintln!("Error changing password: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::RecoveryKey { path, password } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        print!("Enter vault password: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        let _ = std::io::stdin().read_line(&mut p);
                        p.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::unlock_vault(path_buf, &pwd, None) {
                    Ok(master_key) => {
                        let rec_key = vault::export_recovery_key(&master_key);
                        println!("Vault Recovery Key:");
                        println!("  {rec_key}");
                        println!(
                            "\nStore this key safely! It can unlock your vault without a password."
                        );
                    }
                    Err(e) => {
                        eprintln!("Error unlocking vault: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Recover { path, key } => {
                let rec_key = match key {
                    Some(k) => k,
                    None => {
                        print!("Enter recovery key: ");
                        let _ = std::io::stdout().flush();
                        let mut k = String::new();
                        let _ = std::io::stdin().read_line(&mut k);
                        k.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::unlock_vault_with_recovery_key(&rec_key) {
                    Ok(master_key) => match vault::list_files(path_buf, &master_key) {
                        Ok(files) => {
                            println!("Vault successfully decrypted using recovery key!");
                            println!("Found {} file(s) in vault:", files.len());
                            for f in files {
                                println!("  - {f}");
                            }
                        }
                        Err(e) => {
                            eprintln!("Decryption failed with recovery key: {e}");
                            std::process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("Invalid recovery key: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Ls { path, password } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        print!("Enter vault password: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        let _ = std::io::stdin().read_line(&mut p);
                        p.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::unlock_vault(path_buf, &pwd, None) {
                    Ok(master_key) => match vault::list_files(path_buf, &master_key) {
                        Ok(files) => {
                            if files.is_empty() {
                                println!("Vault is empty.");
                            } else {
                                for f in files {
                                    if !f.ends_with("/.keep") {
                                        println!("{f}");
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error listing files: {e}");
                            std::process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("Unlock failed: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Put {
                path,
                source,
                target,
                password,
            } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        print!("Enter vault password: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        let _ = std::io::stdin().read_line(&mut p);
                        p.trim().to_string()
                    }
                };
                let content = match std::fs::read(&source) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to read source file '{source}': {e}");
                        std::process::exit(1);
                    }
                };
                let target_path = target.unwrap_or_else(|| {
                    Path::new(&source)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "imported_file".to_string())
                });
                let path_buf = Path::new(&path);
                match vault::unlock_vault(path_buf, &pwd, None) {
                    Ok(master_key) => {
                        match vault::write_file(path_buf, &master_key, &target_path, &content) {
                            Ok(_) => {
                                println!(
                                    "File stored as '{target_path}' ({}) in vault.",
                                    format_bytes_cli(content.len() as u64)
                                );
                            }
                            Err(e) => {
                                eprintln!("Error storing file: {e}");
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Unlock failed: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Cat {
                path,
                file,
                password,
            } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        print!("Enter vault password: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        let _ = std::io::stdin().read_line(&mut p);
                        p.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::unlock_vault(path_buf, &pwd, None) {
                    Ok(master_key) => match vault::read_file(path_buf, &master_key, &file) {
                        Ok(content) => {
                            let _ = std::io::stdout().write_all(&content);
                        }
                        Err(e) => {
                            eprintln!("Error reading file '{file}': {e}");
                            std::process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("Unlock failed: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Rm {
                path,
                file,
                password,
            } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        print!("Enter vault password: ");
                        let _ = std::io::stdout().flush();
                        let mut p = String::new();
                        let _ = std::io::stdin().read_line(&mut p);
                        p.trim().to_string()
                    }
                };
                let path_buf = Path::new(&path);
                match vault::unlock_vault(path_buf, &pwd, None) {
                    Ok(master_key) => match vault::delete_file(path_buf, &master_key, &file) {
                        Ok(_) => {
                            println!("File '{file}' deleted from vault.");
                        }
                        Err(e) => {
                            eprintln!("Error deleting file: {e}");
                            std::process::exit(1);
                        }
                    },
                    Err(e) => {
                        eprintln!("Unlock failed: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
        }
    }

    #[cfg(feature = "egui-ui")]
    {
        if let Err(e) = egui_ui::run_gui() {
            eprintln!("Error running GUI: {e}");
            std::process::exit(1);
        }
    }

    #[cfg(not(feature = "egui-ui"))]
    {
        eprintln!("GUI is disabled in this build. Use --tui or a CLI command.");
    }
}

fn format_bytes_cli(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
