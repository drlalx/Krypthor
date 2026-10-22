mod cli;
#[cfg(feature = "egui-ui")]
mod egui_ui;
#[cfg(unix)]
mod fs_fuse;
#[cfg(feature = "tui")]
mod tui_app;
mod vault;
#[cfg(feature = "web")]
mod web_server;

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
    if Path::new(CONFIG_FILE).exists() {
        if let Ok(content) = fs::read_to_string(CONFIG_FILE) {
            if let Ok(vaults) = serde_json::from_str::<Vec<SavedVault>>(&content) {
                return vaults;
            }
        }
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
