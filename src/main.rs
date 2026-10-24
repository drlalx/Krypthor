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
            cli::Commands::List { json } => {
                let vaults = load_saved_vaults();
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&vaults).unwrap_or_else(|_| "[]".into())
                    );
                } else if vaults.is_empty() {
                    println!("No registered vaults found.");
                } else {
                    println!("Registered vaults:");
                    for (i, v) in vaults.iter().enumerate() {
                        println!("  [{i}] {} ({})", v.name, v.path);
                    }
                }
                return;
            }
            cli::Commands::Add { name, path } => {
                let path_buf = Path::new(&path);
                if !path_buf.join("masterkey.json").exists() {
                    eprintln!("Error: Directory '{path}' does not contain masterkey.json");
                    std::process::exit(1);
                }
                let mut vaults = load_saved_vaults();
                if vaults.iter().any(|v| v.path == path) {
                    println!("Vault at '{path}' is already registered.");
                    return;
                }
                let vault_name = name.unwrap_or_else(|| {
                    path_buf
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("vault")
                        .to_string()
                });
                vaults.push(SavedVault {
                    name: vault_name.clone(),
                    path: path.clone(),
                });
                save_saved_vaults(&vaults);
                println!("Vault '{vault_name}' successfully registered ({path}).");
                return;
            }
            cli::Commands::Remove { path } => {
                let mut vaults = load_saved_vaults();
                let initial_len = vaults.len();
                vaults.retain(|v| v.path != path);
                if vaults.len() < initial_len {
                    save_saved_vaults(&vaults);
                    println!("Vault '{path}' unregistered successfully.");
                } else {
                    println!("Vault '{path}' was not found in registered vaults.");
                }
                return;
            }
            cli::Commands::RenameVault { path, new_name } => {
                let mut vaults = load_saved_vaults();
                if let Some(v) = vaults.iter_mut().find(|v| v.path == path) {
                    let old_name = v.name.clone();
                    v.name = new_name.clone();
                    save_saved_vaults(&vaults);
                    println!("Vault '{old_name}' renamed to '{new_name}'.");
                } else {
                    eprintln!("Error: Vault '{path}' not found in registered vaults.");
                    std::process::exit(1);
                }
                return;
            }
            cli::Commands::Create {
                name,
                path,
                password,
                keyfile,
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
                match vault::create_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
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
                keyfile,
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
                    match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
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
                    let _ = (path, mountpoint, password, keyfile);
                    eprintln!("FUSE mount is only supported on Unix/Linux platforms.");
                    std::process::exit(1);
                }
            }
            cli::Commands::ChangePassword {
                path,
                old_password,
                new_password,
                old_keyfile,
                new_keyfile,
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
                match vault::change_vault_password(
                    path_buf,
                    &old_pwd,
                    old_keyfile.as_deref().map(Path::new),
                    &new_pwd,
                    new_keyfile.as_deref().map(Path::new),
                ) {
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
            cli::Commands::RecoveryKey {
                path,
                password,
                keyfile,
                json,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => {
                        let rec_key = vault::export_recovery_key(&master_key);
                        if json {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "path": path,
                                    "recovery_key": rec_key
                                })
                            );
                        } else {
                            println!("Vault Recovery Key:");
                            println!("  {rec_key}");
                            println!(
                                "\nStore this key safely! It can unlock your vault without a password."
                            );
                        }
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
            cli::Commands::Ls {
                path,
                password,
                keyfile,
                long,
                pattern,
                json,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => match vault::list_files_with_details(path_buf, &master_key) {
                        Ok(items) => {
                            let pattern_lower = pattern.as_deref().map(str::to_lowercase);
                            let visible: Vec<&vault::FileItem> = items
                                .iter()
                                .filter(|item| !item.path.ends_with("/.keep"))
                                .filter(|item| {
                                    if let Some(ref pat) = pattern_lower {
                                        item.path.to_lowercase().contains(pat)
                                    } else {
                                        true
                                    }
                                })
                                .collect();

                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&visible)
                                        .unwrap_or_else(|_| "[]".into())
                                );
                            } else if visible.is_empty() {
                                if pattern.is_some() {
                                    println!("No matching files found.");
                                } else {
                                    println!("Vault is empty.");
                                }
                            } else {
                                for item in visible {
                                    if long {
                                        let size_str = format_bytes_cli(item.size);
                                        let date_str = item
                                            .modified
                                            .map(vault::format_timestamp)
                                            .unwrap_or_else(|| "—".into());
                                        println!("{size_str:<10} {date_str:<16} {}", item.path);
                                    } else {
                                        println!("{}", item.path);
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
            cli::Commands::Find {
                path,
                query,
                password,
                keyfile,
                long,
                json,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => match vault::find_files(path_buf, &master_key, &query) {
                        Ok(items) => {
                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&items)
                                        .unwrap_or_else(|_| "[]".into())
                                );
                            } else if items.is_empty() {
                                println!("No files matching '{query}' found.");
                            } else {
                                for item in items {
                                    if long {
                                        let size_str = format_bytes_cli(item.size);
                                        let date_str = item
                                            .modified
                                            .map(vault::format_timestamp)
                                            .unwrap_or_else(|| "—".into());
                                        println!("{size_str:<10} {date_str:<16} {}", item.path);
                                    } else {
                                        println!("{}", item.path);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error searching files: {e}");
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
                keyfile,
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
                let src_file = match std::fs::File::open(&source) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Failed to open source file '{source}': {e}");
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => {
                        match vault::write_file_from_reader(
                            path_buf,
                            &master_key,
                            &target_path,
                            src_file,
                        ) {
                            Ok(bytes_written) => {
                                println!(
                                    "File stored as '{target_path}' ({}) in vault.",
                                    format_bytes_cli(bytes_written)
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
                keyfile,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => {
                        let stdout = std::io::stdout();
                        let mut handle = stdout.lock();
                        if let Err(e) =
                            vault::read_file_to_writer(path_buf, &master_key, &file, &mut handle)
                        {
                            eprintln!("Error reading file '{file}': {e}");
                            std::process::exit(1);
                        }
                    }
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
                recursive,
                password,
                keyfile,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => {
                        if recursive {
                            match vault::delete_dir(path_buf, &master_key, &file) {
                                Ok(count) => {
                                    println!("Removed {count} item(s) in '{file}'.");
                                }
                                Err(e) => {
                                    eprintln!("Error deleting directory '{file}': {e}");
                                    std::process::exit(1);
                                }
                            }
                        } else {
                            match vault::delete_file(path_buf, &master_key, &file) {
                                Ok(_) => {
                                    println!("File '{file}' deleted from vault.");
                                }
                                Err(e) => {
                                    eprintln!("Error deleting file: {e}");
                                    std::process::exit(1);
                                }
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
            cli::Commands::Mv {
                path,
                old_path,
                new_path,
                password,
                keyfile,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => {
                        let files = match vault::list_files(path_buf, &master_key) {
                            Ok(f) => f,
                            Err(e) => {
                                eprintln!("Error inspecting vault: {e}");
                                std::process::exit(1);
                            }
                        };
                        let is_dir = files
                            .iter()
                            .any(|f| f.starts_with(&format!("{}/", old_path.trim_matches('/'))));
                        let result = if is_dir {
                            vault::rename_dir(path_buf, &master_key, &old_path, &new_path)
                        } else {
                            vault::rename_file(path_buf, &master_key, &old_path, &new_path)
                        };
                        match result {
                            Ok(_) => {
                                println!(
                                    "Successfully moved/renamed '{old_path}' to '{new_path}'."
                                );
                            }
                            Err(e) => {
                                eprintln!("Error moving file: {e}");
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
            cli::Commands::Cp {
                path,
                src,
                dst,
                recursive,
                password,
                keyfile,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => {
                        let files = match vault::list_files(path_buf, &master_key) {
                            Ok(f) => f,
                            Err(e) => {
                                eprintln!("Error listing files: {e}");
                                std::process::exit(1);
                            }
                        };
                        let clean_src = src.trim().trim_matches('/');
                        let is_dir = files
                            .iter()
                            .any(|f| f.starts_with(&format!("{clean_src}/")));

                        if is_dir {
                            if !recursive {
                                eprintln!(
                                    "Omitting directory '{src}'. Use -r or --recursive to copy directories."
                                );
                                std::process::exit(1);
                            }
                            match vault::copy_dir(path_buf, &master_key, &src, &dst) {
                                Ok(count) => {
                                    println!(
                                        "Successfully copied {count} file(s) from directory '{src}' to '{dst}'."
                                    );
                                }
                                Err(e) => {
                                    eprintln!("Error copying directory: {e}");
                                    std::process::exit(1);
                                }
                            }
                        } else {
                            match vault::copy_file(path_buf, &master_key, &src, &dst) {
                                Ok(_) => {
                                    println!("Successfully copied '{src}' to '{dst}'.");
                                }
                                Err(e) => {
                                    eprintln!("Error copying file: {e}");
                                    std::process::exit(1);
                                }
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
            cli::Commands::Stats {
                path,
                password,
                keyfile,
                json,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => match vault::get_vault_stats(path_buf, &master_key) {
                        Ok(stats) => {
                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&stats)
                                        .unwrap_or_else(|_| "{}".into())
                                );
                            } else {
                                println!("Vault Statistics for '{path}':");
                                println!("  Decrypted Files: {}", stats.total_files);
                                println!(
                                    "  Payload Size:    {}",
                                    format_bytes_cli(stats.total_size_bytes)
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("Error computing stats: {e}");
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
            cli::Commands::Mkdir {
                path,
                folder,
                password,
                keyfile,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => match vault::create_dir(path_buf, &master_key, &folder) {
                        Ok(_) => {
                            println!("Directory '{folder}' created successfully in vault.");
                        }
                        Err(e) => {
                            eprintln!("Error creating directory: {e}");
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
            cli::Commands::Export {
                path,
                dest,
                subfolder,
                password,
                keyfile,
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
                let dest_buf = Path::new(&dest);
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => {
                        match vault::export_vault(
                            path_buf,
                            &master_key,
                            dest_buf,
                            subfolder.as_deref(),
                        ) {
                            Ok(count) => {
                                if let Some(ref sub) = subfolder {
                                    println!(
                                        "Successfully exported {count} decrypted file(s) from subfolder '{sub}' to '{dest}'."
                                    );
                                } else {
                                    println!(
                                        "Successfully exported {count} decrypted file(s) to '{dest}'."
                                    );
                                }
                            }
                            Err(e) => {
                                eprintln!("Error exporting vault: {e}");
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
            cli::Commands::Import {
                path,
                source,
                dest,
                password,
                keyfile,
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
                let src_buf = Path::new(&source);
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => {
                        match vault::import_dir(path_buf, &master_key, src_buf, dest.as_deref()) {
                            Ok(count) => {
                                println!(
                                    "Successfully imported {count} file(s) from '{source}' into vault."
                                );
                            }
                            Err(e) => {
                                eprintln!("Error importing directory: {e}");
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
            cli::Commands::Verify {
                path,
                password,
                keyfile,
                json,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => match vault::verify_vault_integrity(path_buf, &master_key) {
                        Ok(report) => {
                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&report)
                                        .unwrap_or_else(|_| "{}".into())
                                );
                            } else {
                                println!("Vault Integrity Report for '{path}':");
                                if report.is_healthy {
                                    println!(
                                        "  Status:           HEALTHY (All integrity checks passed)"
                                    );
                                } else {
                                    println!(
                                        "  Status:           CORRUPTED (Integrity issues found!)"
                                    );
                                }
                                println!("  Files Checked:    {}", report.total_checked);
                                println!("  Valid Files:      {}", report.valid_files);
                                if !report.corrupted_files.is_empty() {
                                    println!(
                                        "  Corrupted Files ({}):",
                                        report.corrupted_files.len()
                                    );
                                    for c in &report.corrupted_files {
                                        println!("    - {c}");
                                    }
                                }
                                if !report.orphaned_files.is_empty() {
                                    println!("  Orphaned Files ({}):", report.orphaned_files.len());
                                    for o in &report.orphaned_files {
                                        println!("    - {o}");
                                    }
                                }
                            }
                            if !report.is_healthy {
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error verifying vault: {e}");
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
            cli::Commands::BackupKey { path, out } => {
                let path_buf = Path::new(&path);
                match vault::create_masterkey_backup(path_buf, out.as_deref().map(Path::new)) {
                    Ok(dest) => {
                        println!(
                            "Masterkey backup created successfully at '{}'.",
                            dest.display()
                        );
                    }
                    Err(e) => {
                        eprintln!("Error creating masterkey backup: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::RestoreKey { path, from } => {
                let path_buf = Path::new(&path);
                match vault::restore_masterkey_backup(path_buf, from.as_deref().map(Path::new)) {
                    Ok(_) => {
                        let source_str = from.unwrap_or_else(|| "masterkey.json.bkup".to_string());
                        println!(
                            "Masterkey successfully restored from '{source_str}' to '{path}/masterkey.json'."
                        );
                    }
                    Err(e) => {
                        eprintln!("Error restoring masterkey: {e}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Benchmark { json } => {
                let bench = vault::run_benchmark();
                if json {
                    match serde_json::to_string_pretty(&bench) {
                        Ok(json_str) => println!("{json_str}"),
                        Err(e) => {
                            eprintln!("Error formatting JSON: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    println!("⚡ Running Cryptographic Performance Benchmark...\n");
                    println!(
                        "  • Scrypt (Key Derivation):    {:.1} ops/s  ({:.1} ms/op)",
                        bench.scrypt_derivations_per_sec, bench.scrypt_duration_ms
                    );
                    println!(
                        "  • AES-256-GCM Encryption:     {:.1} MB/s",
                        bench.aes_gcm_encrypt_mb_per_sec
                    );
                    println!(
                        "  • AES-256-GCM Decryption:     {:.1} MB/s",
                        bench.aes_gcm_decrypt_mb_per_sec
                    );
                    println!(
                        "  • SHA-256 (Path Obfuscation): {:.0} hashes/s\n",
                        bench.sha256_hashes_per_sec
                    );
                    println!("Total benchmark elapsed: {:.1} ms", bench.total_duration_ms);
                }
                return;
            }
            cli::Commands::Doctor { json } => {
                let saved = load_saved_vaults();
                let vault_paths: Vec<String> = saved.into_iter().map(|v| v.path).collect();
                let report = vault::run_doctor(&vault_paths);

                if json {
                    match serde_json::to_string_pretty(&report) {
                        Ok(json_str) => println!("{json_str}"),
                        Err(e) => {
                            eprintln!("Error formatting JSON: {e}");
                            std::process::exit(1);
                        }
                    }
                } else {
                    println!("🩺 Running Environment & System Diagnostics...\n");
                    for check in &report.checks {
                        let status_tag = match check.status.as_str() {
                            "ok" => "[OK]     ",
                            "warning" => "[WARN]   ",
                            _ => "[ERROR]  ",
                        };
                        println!("{status_tag} {}: {}", check.category, check.name);
                        println!("          {}\n", check.message);
                    }
                    if report.is_healthy {
                        println!(
                            "Status: All system checks passed healthy ({} vault(s) checked).",
                            report.vaults_checked
                        );
                    } else {
                        eprintln!(
                            "Status: Potential issues detected ({} vault(s) checked).",
                            report.vaults_checked
                        );
                        std::process::exit(1);
                    }
                }
                return;
            }
            cli::Commands::Hash {
                path,
                file,
                password,
                keyfile,
                json,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => match vault::hash_file(path_buf, &master_key, &file) {
                        Ok(res) => {
                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&res)
                                        .unwrap_or_else(|_| "{}".into())
                                );
                            } else {
                                println!("File:   {}", res.file);
                                println!("Size:   {}", format_bytes_cli(res.size_bytes));
                                println!("SHA256: {}", res.sha256);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error computing hash: {e}");
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
            cli::Commands::Prune {
                path,
                dry_run,
                password,
                keyfile,
                json,
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
                match vault::unlock_vault(path_buf, &pwd, keyfile.as_deref().map(Path::new)) {
                    Ok(master_key) => match vault::prune_vault(path_buf, &master_key, dry_run) {
                        Ok(report) => {
                            if json {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&report)
                                        .unwrap_or_else(|_| "{}".into())
                                );
                            } else {
                                println!("Pruning Vault '{}' (dry_run: {}):", path, report.dry_run);
                                println!("  Storage files scanned: {}", report.files_scanned);
                                println!("  Orphaned files found:  {}", report.pruned_files.len());
                                println!(
                                    "  Space reclaimed:       {}",
                                    format_bytes_cli(report.bytes_reclaimed)
                                );
                                for f in &report.pruned_files {
                                    println!("    - {f}");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error pruning vault: {e}");
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
