use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use scrypt::{Params, scrypt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static WRITE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize, Debug)]
pub struct MasterKeyFile {
    pub salt: String,                 // Hex encoded salt
    pub encrypted_master_key: String, // Hex encoded (nonce + ciphertext)
    pub keyfile_required: Option<bool>,
}

// Derive a 32-byte key from password and salt using scrypt
pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    // Standard parameters: N=16384 (log_n=14), r=8, p=1, len=32
    let params = Params::new(14, 8, 1, 32).map_err(|e| e.to_string())?;
    let mut derived_key = [0u8; 32];
    scrypt(password.as_bytes(), salt, &params, &mut derived_key).map_err(|e| e.to_string())?;
    Ok(derived_key)
}

// Derive final key combining passphrase and optional keyfile
pub fn derive_final_key(
    password: &str,
    salt: &[u8],
    keyfile_path: Option<&Path>,
) -> Result<[u8; 32], String> {
    let base_key = derive_key(password, salt)?;
    if let Some(path) = keyfile_path {
        let keyfile_bytes = fs::read(path).map_err(|e| format!("Failed to read keyfile: {}", e))?;
        let mut hasher = Sha256::new();
        hasher.update(&keyfile_bytes);
        let keyfile_hash = hasher.finalize();

        let mut final_hasher = Sha256::new();
        final_hasher.update(base_key);
        final_hasher.update(keyfile_hash);
        let final_hash = final_hasher.finalize();

        let mut final_key = [0u8; 32];
        final_key.copy_from_slice(&final_hash);
        Ok(final_key)
    } else {
        Ok(base_key)
    }
}

// Decrypt filename by trying to match or decrypt using candidate paths, or since we list files,
// we can store the plain filename in the metadata header of the file!
// Let's store the plain relative path inside the encrypted file header!
// This is much better because it makes listing files extremely robust and independent of guessing paths!
// Inside the encrypted file format:
// [12 bytes nonce] [4 bytes relative path length] [Encrypted relative path] [Encrypted content]
// This is beautiful! It allows us to:
// 1. Instantly read any file by its path because the physical filename is SHA256(relative_path) or deterministic encrypted name.
// 2. Easily list all files in the vault by scanning the directory and decrypting the relative path stored in the header of each file!
// Let's implement this file structure:
// Header:
// - Nonce (12 bytes)
// - Path Length (4 bytes, u32 in big endian)
// - Encrypted Relative Path (using the master_key directly with the file's nonce)
// - Encrypted Content (using the master_key directly with the file's nonce)
// Note: We can encrypt both the path and content using the same 12-byte random nonce, which is safe because they are separate ciphertext blocks. To make it even simpler, we can just serialize a JSON metadata struct, encrypt it, prepended by the nonce, and write it to the file!
// Let's do that! A serialized struct containing:
// { "path": "relative/path/to/file", "content": "..." }
// Or:
// { "path": "...", "data": "base64_or_hex_data" }
// For large files, base64 encoding might consume 33% more memory/storage, which is fine for a vault up to tens of megabytes.
// To support large files efficiently and simply without base64 overhead, we can encrypt the path and content as a binary payload:
// [12 bytes nonce] [4 bytes path_len] [encrypted_path] [encrypted_content]
// Let's write the code for this binary format! It is clean, efficient, and professional.

pub fn encrypt_file_payload(
    master_key: &[u8; 32],
    relative_path: &str,
    content: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(master_key));

    // Generate a random 12-byte nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt the path
    let encrypted_path = cipher
        .encrypt(nonce, relative_path.as_bytes())
        .map_err(|e| format!("Path encryption failed: {}", e))?;

    // Encrypt the content
    let encrypted_content = cipher
        .encrypt(nonce, content)
        .map_err(|e| format!("Content encryption failed: {}", e))?;

    let mut payload = Vec::new();
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&(encrypted_path.len() as u32).to_be_bytes());
    payload.extend_from_slice(&encrypted_path);
    payload.extend_from_slice(&encrypted_content);

    Ok(payload)
}

pub fn decrypt_file_payload(
    master_key: &[u8; 32],
    payload: &[u8],
) -> Result<(String, Vec<u8>), String> {
    if payload.len() < 16 {
        return Err("Payload too short".to_string());
    }

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(master_key));

    let nonce_bytes = &payload[0..12];
    let nonce = Nonce::from_slice(nonce_bytes);

    let path_len =
        u32::from_be_bytes([payload[12], payload[13], payload[14], payload[15]]) as usize;
    if payload.len() < 16 + path_len {
        return Err("Invalid payload length".to_string());
    }

    let encrypted_path = &payload[16..16 + path_len];
    let encrypted_content = &payload[16 + path_len..];

    let decrypted_path_bytes = cipher
        .decrypt(nonce, encrypted_path)
        .map_err(|e| format!("Path decryption failed: {}", e))?;
    let relative_path = String::from_utf8(decrypted_path_bytes)
        .map_err(|e| format!("Invalid UTF-8 in decrypted path: {}", e))?;

    let decrypted_content = cipher
        .decrypt(nonce, encrypted_content)
        .map_err(|e| format!("Content decryption failed: {}", e))?;

    Ok((relative_path, decrypted_content))
}

// Generate the physical filename (obfuscated) deterministic from the relative path
pub fn get_physical_filename(master_key: &[u8; 32], relative_path: &str) -> String {
    // We can use Sha256(master_key || relative_path)
    let mut hasher = Sha256::new();
    hasher.update(master_key);
    hasher.update(relative_path.as_bytes());
    let result = hasher.finalize();
    format!("{}.c9r", hex::encode(result))
}

pub fn create_vault(
    vault_dir: &Path,
    password: &str,
    keyfile_path: Option<&Path>,
) -> Result<(), String> {
    println!("[DEBUG] create_vault: Starting for dir: {:?}", vault_dir);

    if !vault_dir.exists() {
        println!("[DEBUG] create_vault: Creating vault_dir...");
        fs::create_dir_all(vault_dir)
            .map_err(|e| format!("fs::create_dir_all(vault_dir) failed: {}", e))?;
    }

    let d_dir = vault_dir.join("d");
    if !d_dir.exists() {
        println!("[DEBUG] create_vault: Creating d_dir: {:?}", d_dir);
        fs::create_dir_all(&d_dir)
            .map_err(|e| format!("fs::create_dir_all(d_dir) failed: {}", e))?;
    }

    // Generate salt
    println!("[DEBUG] create_vault: Generating salt...");
    let mut salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt);

    // Derive key combining password and keyfile
    println!("[DEBUG] create_vault: Deriving key...");
    let derived_key = derive_final_key(password, &salt, keyfile_path)?;

    // Generate master key
    println!("[DEBUG] create_vault: Generating master key...");
    let mut master_key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut master_key);

    // Encrypt master key with derived key
    println!("[DEBUG] create_vault: Encrypting master key...");
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted = cipher
        .encrypt(nonce, master_key.as_ref())
        .map_err(|e| format!("Master key encryption failed: {}", e))?;

    let mut encrypted_payload = Vec::new();
    encrypted_payload.extend_from_slice(&nonce_bytes);
    encrypted_payload.extend_from_slice(&encrypted);

    let key_file = MasterKeyFile {
        salt: hex::encode(salt),
        encrypted_master_key: hex::encode(encrypted_payload),
        keyfile_required: Some(keyfile_path.is_some()),
    };

    let key_file_path = vault_dir.join("masterkey.json");
    let key_file_tmp = vault_dir.join("masterkey.json.tmp");
    println!("[DEBUG] create_vault: Serializing masterkey.json content...");
    let key_file_content = serde_json::to_string_pretty(&key_file)
        .map_err(|e| format!("serde_json::to_string_pretty failed: {}", e))?;

    println!(
        "[DEBUG] create_vault: Writing masterkey.json at {:?}",
        key_file_path
    );
    fs::write(&key_file_tmp, key_file_content.as_bytes())
        .map_err(|e| format!("fs::write(masterkey.json.tmp) failed: {}", e))?;
    fs::rename(&key_file_tmp, &key_file_path)
        .map_err(|e| format!("fs::rename(masterkey.json) failed: {}", e))?;

    // Create initial backup
    let bkup_path = vault_dir.join("masterkey.json.bkup");
    let _ = fs::copy(&key_file_path, &bkup_path);

    // Create an empty vault config atomically
    let vault_config_path = vault_dir.join("vault.json");
    let vault_config_tmp = vault_dir.join("vault.json.tmp");
    println!(
        "[DEBUG] create_vault: Writing vault.json at {:?}",
        vault_config_path
    );
    fs::write(&vault_config_tmp, b"{\"version\": 8}")
        .map_err(|e| format!("fs::write(vault.json.tmp) failed: {}", e))?;
    fs::rename(&vault_config_tmp, &vault_config_path)
        .map_err(|e| format!("fs::rename(vault.json) failed: {}", e))?;

    println!("[DEBUG] create_vault: Finished successfully!");
    Ok(())
}

pub fn change_vault_password(
    vault_dir: &Path,
    old_password: &str,
    old_keyfile: Option<&Path>,
    new_password: &str,
    new_keyfile: Option<&Path>,
) -> Result<(), String> {
    // 1. Decrypt existing master key
    let master_key = unlock_vault(vault_dir, old_password, old_keyfile)?;

    // 2. Generate new salt
    let mut new_salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut new_salt);

    // 3. Derive new key combining new password and new keyfile
    let derived_key = derive_final_key(new_password, &new_salt, new_keyfile)?;

    // 4. Re-encrypt the existing master key
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted = cipher
        .encrypt(nonce, master_key.as_ref())
        .map_err(|e| format!("Master key re-encryption failed: {}", e))?;

    let mut encrypted_payload = Vec::new();
    encrypted_payload.extend_from_slice(&nonce_bytes);
    encrypted_payload.extend_from_slice(&encrypted);

    let key_file = MasterKeyFile {
        salt: hex::encode(new_salt),
        encrypted_master_key: hex::encode(encrypted_payload),
        keyfile_required: Some(new_keyfile.is_some()),
    };

    let key_file_path = vault_dir.join("masterkey.json");
    let bkup_path = vault_dir.join("masterkey.json.bkup");
    if key_file_path.exists() {
        let _ = fs::copy(&key_file_path, &bkup_path);
    }

    let key_file_tmp = vault_dir.join("masterkey.json.tmp");
    let key_file_content = serde_json::to_string_pretty(&key_file)
        .map_err(|e| format!("serde_json::to_string_pretty failed: {}", e))?;

    fs::write(&key_file_tmp, key_file_content.as_bytes())
        .map_err(|e| format!("fs::write(masterkey.json.tmp) failed: {}", e))?;
    fs::rename(&key_file_tmp, &key_file_path)
        .map_err(|e| format!("fs::rename(masterkey.json) failed: {}", e))?;

    Ok(())
}

pub fn load_master_key_file(vault_dir: &Path) -> Result<MasterKeyFile, String> {
    let key_file_path = vault_dir.join("masterkey.json");
    let bkup_path = vault_dir.join("masterkey.json.bkup");

    let parse_file = |path: &Path| -> Result<MasterKeyFile, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
    };

    if key_file_path.exists() {
        match parse_file(&key_file_path) {
            Ok(kf) => Ok(kf),
            Err(primary_err) => {
                if bkup_path.exists() {
                    parse_file(&bkup_path).map_err(|bkup_err| {
                        format!("Failed to parse masterkey.json ({primary_err}) and masterkey.json.bkup ({bkup_err})")
                    })
                } else {
                    Err(primary_err)
                }
            }
        }
    } else if bkup_path.exists() {
        parse_file(&bkup_path)
    } else {
        Err("masterkey.json not found".to_string())
    }
}

pub fn export_recovery_key(master_key: &[u8; 32]) -> String {
    let hex_key = hex::encode_upper(master_key);
    hex_key
        .as_bytes()
        .chunks(4)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("-")
}

pub fn unlock_vault_with_recovery_key(recovery_key: &str) -> Result<[u8; 32], String> {
    let clean = recovery_key.replace(['-', ' ', '\n', '\t'], "");
    let bytes = hex::decode(clean).map_err(|e| format!("Invalid recovery key format: {e}"))?;
    if bytes.len() != 32 {
        return Err("Recovery key must be 32 bytes (64 hex characters)".into());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

pub fn unlock_vault(
    vault_dir: &Path,
    password: &str,
    keyfile_path: Option<&Path>,
) -> Result<[u8; 32], String> {
    let key_file = load_master_key_file(vault_dir)?;

    let keyfile_req = key_file.keyfile_required.unwrap_or(false);
    if keyfile_req && keyfile_path.is_none() {
        return Err("Keyfile is required for this vault.".to_string());
    }
    if !keyfile_req && keyfile_path.is_some() {
        return Err("This vault does not use a keyfile.".to_string());
    }

    let salt = hex::decode(&key_file.salt).map_err(|e| format!("Invalid salt encoding: {}", e))?;

    let derived_key = derive_final_key(password, &salt, keyfile_path)?;

    let encrypted_payload = hex::decode(&key_file.encrypted_master_key)
        .map_err(|e| format!("Invalid encrypted master key encoding: {}", e))?;

    if encrypted_payload.len() < 12 {
        return Err("Invalid encrypted master key payload".to_string());
    }

    let nonce_bytes = &encrypted_payload[0..12];
    let ciphertext = &encrypted_payload[12..];

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));
    let nonce = Nonce::from_slice(nonce_bytes);

    let decrypted = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Incorrect password or keyfile".to_string())?;

    if decrypted.len() != 32 {
        return Err("Decrypted key size is incorrect".to_string());
    }

    let mut master_key = [0u8; 32];
    master_key.copy_from_slice(&decrypted);

    Ok(master_key)
}

pub fn write_file(
    vault_dir: &Path,
    master_key: &[u8; 32],
    relative_path: &str,
    content: &[u8],
) -> Result<(), String> {
    let phys_name = get_physical_filename(master_key, relative_path);
    let phys_path = vault_dir.join("d").join(&phys_name);

    let payload = encrypt_file_payload(master_key, relative_path, content)?;

    let count = WRITE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = vault_dir
        .join("d")
        .join(format!("{phys_name}.tmp.{}.{count}", std::process::id()));

    fs::write(&tmp_path, &payload).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &phys_path).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn read_file(
    vault_dir: &Path,
    master_key: &[u8; 32],
    relative_path: &str,
) -> Result<Vec<u8>, String> {
    let phys_name = get_physical_filename(master_key, relative_path);
    let phys_path = vault_dir.join("d").join(phys_name);

    if !phys_path.exists() {
        return Err("File not found in vault".to_string());
    }

    let mut file = File::open(phys_path).map_err(|e| e.to_string())?;
    let mut payload = Vec::new();
    file.read_to_end(&mut payload).map_err(|e| e.to_string())?;

    let (_, content) = decrypt_file_payload(master_key, &payload)?;
    Ok(content)
}

pub fn delete_file(
    vault_dir: &Path,
    master_key: &[u8; 32],
    relative_path: &str,
) -> Result<(), String> {
    let phys_name = get_physical_filename(master_key, relative_path);
    let phys_path = vault_dir.join("d").join(phys_name);

    if phys_path.exists() {
        fs::remove_file(phys_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn rename_file(
    vault_dir: &Path,
    master_key: &[u8; 32],
    old_rel_path: &str,
    new_rel_path: &str,
) -> Result<(), String> {
    if old_rel_path == new_rel_path {
        return Ok(());
    }
    let content = read_file(vault_dir, master_key, old_rel_path)?;
    write_file(vault_dir, master_key, new_rel_path, &content)?;
    delete_file(vault_dir, master_key, old_rel_path)?;
    Ok(())
}

pub fn rename_dir(
    vault_dir: &Path,
    master_key: &[u8; 32],
    old_prefix: &str,
    new_prefix: &str,
) -> Result<(), String> {
    let old_p = old_prefix.trim().trim_matches('/');
    let new_p = new_prefix.trim().trim_matches('/');
    if old_p == new_p {
        return Ok(());
    }
    let files = list_files(vault_dir, master_key)?;
    let old_with_slash = format!("{old_p}/");

    for file in files {
        if let Some(suffix) = file.strip_prefix(&old_with_slash) {
            let new_path = format!("{new_p}/{suffix}");
            rename_file(vault_dir, master_key, &file, &new_path)?;
        } else if file == old_p {
            rename_file(vault_dir, master_key, &file, new_p)?;
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileItem {
    pub path: String,
    pub size: u64,
}

pub fn list_files_with_details(
    vault_dir: &Path,
    master_key: &[u8; 32],
) -> Result<Vec<FileItem>, String> {
    let d_dir = vault_dir.join("d");
    if !d_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(d_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "c9r") {
            let mut file = File::open(&path).map_err(|e| e.to_string())?;
            let mut payload = Vec::new();
            if file.read_to_end(&mut payload).is_ok()
                && let Ok((rel_path, content)) = decrypt_file_payload(master_key, &payload)
            {
                files.push(FileItem {
                    path: rel_path,
                    size: content.len() as u64,
                });
            }
        }
    }

    // Sort files alphabetically by path
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

pub fn list_files(vault_dir: &Path, master_key: &[u8; 32]) -> Result<Vec<String>, String> {
    Ok(list_files_with_details(vault_dir, master_key)?
        .into_iter()
        .map(|item| item.path)
        .collect())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct VaultStats {
    pub total_files: usize,
    pub total_size_bytes: u64,
}

pub fn get_vault_stats(vault_dir: &Path, master_key: &[u8; 32]) -> Result<VaultStats, String> {
    let d_dir = vault_dir.join("d");
    if !d_dir.exists() {
        return Ok(VaultStats {
            total_files: 0,
            total_size_bytes: 0,
        });
    }

    let mut total_files = 0;
    let mut total_size_bytes = 0;

    for entry in fs::read_dir(d_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "c9r") {
            let mut file = File::open(&path).map_err(|e| e.to_string())?;
            let mut payload = Vec::new();
            if file.read_to_end(&mut payload).is_ok()
                && let Ok((_rel_path, content)) = decrypt_file_payload(master_key, &payload)
            {
                total_files += 1;
                total_size_bytes += content.len() as u64;
            }
        }
    }

    Ok(VaultStats {
        total_files,
        total_size_bytes,
    })
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct IntegrityReport {
    pub total_checked: usize,
    pub valid_files: usize,
    pub corrupted_files: Vec<String>,
    pub orphaned_files: Vec<String>,
    pub is_healthy: bool,
}

pub fn verify_vault_integrity(
    vault_dir: &Path,
    master_key: &[u8; 32],
) -> Result<IntegrityReport, String> {
    let d_dir = vault_dir.join("d");
    if !d_dir.exists() {
        return Ok(IntegrityReport {
            total_checked: 0,
            valid_files: 0,
            corrupted_files: Vec::new(),
            orphaned_files: Vec::new(),
            is_healthy: true,
        });
    }

    let mut total_checked = 0;
    let mut valid_files = 0;
    let mut corrupted_files = Vec::new();
    let mut orphaned_files = Vec::new();

    for entry in fs::read_dir(d_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if !file_name.ends_with(".c9r") {
                orphaned_files.push(file_name);
                continue;
            }

            total_checked += 1;
            let mut file = match File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    corrupted_files.push(format!("{file_name}: Failed to open ({e})"));
                    continue;
                }
            };

            let mut payload = Vec::new();
            if let Err(e) = file.read_to_end(&mut payload) {
                corrupted_files.push(format!("{file_name}: Read error ({e})"));
                continue;
            }

            match decrypt_file_payload(master_key, &payload) {
                Ok((rel_path, _)) => {
                    let expected_name = get_physical_filename(master_key, &rel_path);
                    if expected_name != file_name {
                        corrupted_files.push(format!(
                            "{file_name}: Name mismatch (expected {expected_name} for '{rel_path}')"
                        ));
                    } else {
                        valid_files += 1;
                    }
                }
                Err(e) => {
                    corrupted_files.push(format!("{file_name}: Integrity/Decryption failed ({e})"));
                }
            }
        }
    }

    let is_healthy = corrupted_files.is_empty() && orphaned_files.is_empty();

    Ok(IntegrityReport {
        total_checked,
        valid_files,
        corrupted_files,
        orphaned_files,
        is_healthy,
    })
}

pub fn create_dir(
    vault_dir: &Path,
    master_key: &[u8; 32],
    folder_path: &str,
) -> Result<(), String> {
    let trimmed = folder_path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err("Folder name cannot be empty".to_string());
    }
    let keep_file = format!("{trimmed}/.keep");
    write_file(vault_dir, master_key, &keep_file, b"")
}

pub fn export_vault(
    vault_dir: &Path,
    master_key: &[u8; 32],
    dest_dir: &Path,
) -> Result<usize, String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let files = list_files(vault_dir, master_key)?;
    let mut exported_count = 0;

    for rel_path in files {
        if rel_path.ends_with("/.keep") {
            let parent = rel_path.trim_end_matches("/.keep");
            let target_folder = dest_dir.join(parent);
            fs::create_dir_all(&target_folder).map_err(|e| e.to_string())?;
        } else {
            let content = read_file(vault_dir, master_key, &rel_path)?;
            let dest_file_path = dest_dir.join(&rel_path);
            if let Some(parent) = dest_file_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&dest_file_path, &content).map_err(|e| e.to_string())?;
            exported_count += 1;
        }
    }

    Ok(exported_count)
}

pub fn import_dir(
    vault_dir: &Path,
    master_key: &[u8; 32],
    src_dir: &Path,
    dest_prefix: Option<&str>,
) -> Result<usize, String> {
    if !src_dir.is_dir() {
        return Err(format!("'{}' is not a directory", src_dir.display()));
    }

    fn walk_and_import(
        vault_dir: &Path,
        master_key: &[u8; 32],
        current_dir: &Path,
        base_dir: &Path,
        dest_prefix: Option<&str>,
    ) -> Result<usize, String> {
        let mut count = 0;
        for entry in fs::read_dir(current_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                count += walk_and_import(vault_dir, master_key, &path, base_dir, dest_prefix)?;
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(base_dir)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let target_path = match dest_prefix {
                    Some(prefix) if !prefix.trim().trim_matches('/').is_empty() => {
                        format!("{}/{}", prefix.trim().trim_matches('/'), rel)
                    }
                    _ => rel,
                };
                let content = fs::read(&path).map_err(|e| e.to_string())?;
                write_file(vault_dir, master_key, &target_path, &content)?;
                count += 1;
            }
        }
        Ok(count)
    }

    walk_and_import(vault_dir, master_key, src_dir, src_dir, dest_prefix)
}

#[cfg(feature = "web")]
pub fn export_vault_zip(vault_dir: &Path, master_key: &[u8; 32]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip_writer = zip::ZipWriter::new(&mut cursor);
        let files = list_files(vault_dir, master_key)?;
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for rel_path in files {
            if rel_path.ends_with("/.keep") {
                let parent = rel_path.trim_end_matches("/.keep");
                let dir_entry = format!("{parent}/");
                zip_writer
                    .add_directory(&dir_entry, SimpleFileOptions::default())
                    .map_err(|e| e.to_string())?;
            } else {
                let content = read_file(vault_dir, master_key, &rel_path)?;
                zip_writer
                    .start_file(&rel_path, options)
                    .map_err(|e| e.to_string())?;
                zip_writer.write_all(&content).map_err(|e| e.to_string())?;
            }
        }
        zip_writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use std::fs;

    #[test]
    fn test_vault_lifecycle() {
        let test_dir = std::env::temp_dir().join("cryptomator_test_vault");
        if test_dir.exists() {
            let _ = fs::remove_dir_all(&test_dir);
        }

        // Test create
        create_vault(&test_dir, "my_super_secret_password", None).unwrap();

        // Test unlock success
        let key = unlock_vault(&test_dir, "my_super_secret_password", None).unwrap();
        assert_eq!(key.len(), 32);

        // Test unlock failure (wrong password)
        let err = unlock_vault(&test_dir, "wrong_password", None);
        assert!(err.is_err());
        assert_eq!(err.unwrap_err(), "Incorrect password or keyfile");

        // Test write and read file
        let content = b"Hello world, this is a secret file content!";
        write_file(&test_dir, &key, "secrets/hello.txt", content).unwrap();

        let read_content = read_file(&test_dir, &key, "secrets/hello.txt").unwrap();
        assert_eq!(read_content, content);

        // Test list files
        let files = list_files(&test_dir, &key).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "secrets/hello.txt");

        // Test delete
        delete_file(&test_dir, &key, "secrets/hello.txt").unwrap();
        let files_after = list_files(&test_dir, &key).unwrap();
        assert_eq!(files_after.len(), 0);

        // Cleanup
        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_change_vault_password() {
        let test_dir = std::env::temp_dir().join("cryptomator_test_change_pw");
        if test_dir.exists() {
            let _ = fs::remove_dir_all(&test_dir);
        }

        // 1. Create vault with initial password
        create_vault(&test_dir, "initial_password", None).unwrap();
        let key1 = unlock_vault(&test_dir, "initial_password", None).unwrap();
        write_file(&test_dir, &key1, "notes.txt", b"important notes").unwrap();

        // 2. Change password to new_password
        change_vault_password(
            &test_dir,
            "initial_password",
            None,
            "new_secure_password",
            None,
        )
        .unwrap();

        // 3. Old password now fails
        assert!(unlock_vault(&test_dir, "initial_password", None).is_err());

        // 4. New password unlocks same master key and reads existing files!
        let key2 = unlock_vault(&test_dir, "new_secure_password", None).unwrap();
        assert_eq!(key1, key2);
        assert_eq!(
            read_file(&test_dir, &key2, "notes.txt").unwrap(),
            b"important notes"
        );

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_recovery_key() {
        let mut master_key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut master_key);

        let exported = export_recovery_key(&master_key);
        assert!(exported.contains('-'));

        let recovered = unlock_vault_with_recovery_key(&exported).unwrap();
        assert_eq!(master_key, recovered);
    }

    #[test]
    fn test_rename_file_and_dir() {
        let test_dir = std::env::temp_dir().join("cryptomator_test_rename");
        if test_dir.exists() {
            let _ = fs::remove_dir_all(&test_dir);
        }

        create_vault(&test_dir, "testpass", None).unwrap();
        let key = unlock_vault(&test_dir, "testpass", None).unwrap();

        // 1. Write file
        write_file(&test_dir, &key, "docs/old_name.txt", b"secret text").unwrap();
        assert_eq!(
            list_files(&test_dir, &key).unwrap(),
            vec!["docs/old_name.txt"]
        );

        // 2. Rename file
        rename_file(&test_dir, &key, "docs/old_name.txt", "docs/new_name.txt").unwrap();
        assert_eq!(
            list_files(&test_dir, &key).unwrap(),
            vec!["docs/new_name.txt"]
        );
        assert_eq!(
            read_file(&test_dir, &key, "docs/new_name.txt").unwrap(),
            b"secret text"
        );

        // 3. Rename directory
        rename_dir(&test_dir, &key, "docs", "archive").unwrap();
        assert_eq!(
            list_files(&test_dir, &key).unwrap(),
            vec!["archive/new_name.txt"]
        );
        assert_eq!(
            read_file(&test_dir, &key, "archive/new_name.txt").unwrap(),
            b"secret text"
        );

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_vault_with_keyfile() {
        let test_dir = std::env::temp_dir().join("cryptomator_test_vault_keyfile");
        let keyfile_path = std::env::temp_dir().join("test_keyfile.key");
        let wrong_keyfile_path = std::env::temp_dir().join("wrong_keyfile.key");

        if test_dir.exists() {
            let _ = fs::remove_dir_all(&test_dir);
        }
        let _ = fs::write(&keyfile_path, b"some random keyfile content 12345");
        let _ = fs::write(&wrong_keyfile_path, b"some other wrong keyfile content");

        // 1. Create with keyfile
        create_vault(&test_dir, "mypassword", Some(&keyfile_path)).unwrap();

        // 2. Unlock with correct password and keyfile
        let key = unlock_vault(&test_dir, "mypassword", Some(&keyfile_path)).unwrap();
        assert_eq!(key.len(), 32);

        // 3. Unlock fails with wrong keyfile
        let err1 = unlock_vault(&test_dir, "mypassword", Some(&wrong_keyfile_path));
        assert!(err1.is_err());
        assert_eq!(err1.unwrap_err(), "Incorrect password or keyfile");

        // 4. Unlock fails with missing keyfile parameter
        let err2 = unlock_vault(&test_dir, "mypassword", None);
        assert!(err2.is_err());
        assert_eq!(err2.unwrap_err(), "Keyfile is required for this vault.");

        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::remove_file(&keyfile_path);
        let _ = fs::remove_file(&wrong_keyfile_path);
    }

    #[test]
    fn test_derive_key() {
        let salt = [0u8; 32];
        let key1 = derive_key("password123", &salt).unwrap();
        let key2 = derive_key("password123", &salt).unwrap();
        assert_eq!(key1, key2);

        let key3 = derive_key("different_pwd", &salt).unwrap();
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_derive_final_key() {
        let salt = [0u8; 32];
        let keyfile_path = std::env::temp_dir().join("unit_test_keyfile.key");
        fs::write(&keyfile_path, b"keyfile bytes 98765").unwrap();

        let base_key = derive_key("pwd", &salt).unwrap();
        let key_without_file = derive_final_key("pwd", &salt, None).unwrap();
        assert_eq!(base_key, key_without_file);

        let key_with_file = derive_final_key("pwd", &salt, Some(&keyfile_path)).unwrap();
        assert_ne!(key_without_file, key_with_file);

        let _ = fs::remove_file(&keyfile_path);
    }

    #[test]
    fn test_encrypt_decrypt_file_payload() {
        let master_key = [7u8; 32];
        let path = "folder/file.txt";
        let content = b"Super secret text content";

        let encrypted = encrypt_file_payload(&master_key, path, content).unwrap();
        assert!(encrypted.len() > 16);

        let (decrypted_path, decrypted_content) =
            decrypt_file_payload(&master_key, &encrypted).unwrap();
        assert_eq!(decrypted_path, path);
        assert_eq!(decrypted_content, content);
    }

    #[test]
    fn test_get_physical_filename() {
        let master_key = [1u8; 32];
        let path1 = "documents/tax.pdf";
        let name1 = get_physical_filename(&master_key, path1);
        let name2 = get_physical_filename(&master_key, path1);
        assert_eq!(name1, name2);
        assert!(name1.ends_with(".c9r"));

        let path2 = "documents/other.pdf";
        let name3 = get_physical_filename(&master_key, path2);
        assert_ne!(name1, name3);
    }

    #[test]
    fn test_vault_stats() {
        let test_dir = std::env::temp_dir().join("cryptomator_test_stats");
        if test_dir.exists() {
            let _ = fs::remove_dir_all(&test_dir);
        }

        create_vault(&test_dir, "test_pw", None).unwrap();
        let key = unlock_vault(&test_dir, "test_pw", None).unwrap();

        let empty_stats = get_vault_stats(&test_dir, &key).unwrap();
        assert_eq!(empty_stats.total_files, 0);
        assert_eq!(empty_stats.total_size_bytes, 0);

        write_file(&test_dir, &key, "file1.txt", b"hello").unwrap();
        write_file(&test_dir, &key, "sub/file2.txt", b"world 123").unwrap();

        let stats = get_vault_stats(&test_dir, &key).unwrap();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.total_size_bytes, 5 + 9);

        let details = list_files_with_details(&test_dir, &key).unwrap();
        assert_eq!(details.len(), 2);
        assert_eq!(details[0].path, "file1.txt");
        assert_eq!(details[0].size, 5);
        assert_eq!(details[1].path, "sub/file2.txt");
        assert_eq!(details[1].size, 9);

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_create_dir_import_export() {
        let test_dir = std::env::temp_dir().join("cryptomator_import_export_test");
        let src_dir = std::env::temp_dir().join("cryptomator_test_src_tree");
        let dest_dir = std::env::temp_dir().join("cryptomator_test_dest_tree");

        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dest_dir);

        create_vault(&test_dir, "export_pw", None).unwrap();
        let key = unlock_vault(&test_dir, "export_pw", None).unwrap();

        // 1. Test create_dir
        create_dir(&test_dir, &key, "empty_folder").unwrap();
        let files = list_files(&test_dir, &key).unwrap();
        assert!(files.contains(&"empty_folder/.keep".to_string()));

        // 2. Prepare local files for import
        fs::create_dir_all(src_dir.join("nested/folder")).unwrap();
        fs::write(src_dir.join("doc1.txt"), b"Document 1 Content").unwrap();
        fs::write(
            src_dir.join("nested/folder/doc2.txt"),
            b"Document 2 Content",
        )
        .unwrap();

        // 3. Import directory into vault
        let imported = import_dir(&test_dir, &key, &src_dir, Some("backup")).unwrap();
        assert_eq!(imported, 2);

        let files = list_files(&test_dir, &key).unwrap();
        assert!(files.contains(&"backup/doc1.txt".to_string()));
        assert!(files.contains(&"backup/nested/folder/doc2.txt".to_string()));

        // 4. Export entire vault to dest_dir
        let exported = export_vault(&test_dir, &key, &dest_dir).unwrap();
        assert_eq!(exported, 2);

        assert_eq!(
            fs::read(dest_dir.join("backup/doc1.txt")).unwrap(),
            b"Document 1 Content"
        );
        assert_eq!(
            fs::read(dest_dir.join("backup/nested/folder/doc2.txt")).unwrap(),
            b"Document 2 Content"
        );
        assert!(dest_dir.join("empty_folder").is_dir());

        let _ = fs::remove_dir_all(&test_dir);
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_dir_all(&dest_dir);
    }

    #[cfg(feature = "web")]
    #[test]
    fn test_export_vault_zip() {
        let test_dir = std::env::temp_dir().join("cryptomator_zip_test");
        let _ = fs::remove_dir_all(&test_dir);

        create_vault(&test_dir, "zip_pw", None).unwrap();
        let key = unlock_vault(&test_dir, "zip_pw", None).unwrap();

        write_file(&test_dir, &key, "readme.txt", b"Hello ZIP").unwrap();
        create_dir(&test_dir, &key, "docs").unwrap();
        write_file(&test_dir, &key, "docs/manual.md", b"# Manual").unwrap();

        let zip_bytes = export_vault_zip(&test_dir, &key).unwrap();
        assert!(!zip_bytes.is_empty());

        // Read back with ZipArchive
        use std::io::Read;
        let cursor = std::io::Cursor::new(zip_bytes);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        assert!(archive.len() >= 2);

        let mut content = Vec::new();
        {
            let mut f1 = archive.by_name("readme.txt").unwrap();
            f1.read_to_end(&mut content).unwrap();
        }
        assert_eq!(content, b"Hello ZIP");

        let mut content2 = Vec::new();
        {
            let mut f2 = archive.by_name("docs/manual.md").unwrap();
            f2.read_to_end(&mut content2).unwrap();
        }
        assert_eq!(content2, b"# Manual");

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_verify_vault_integrity() {
        let test_dir = std::env::temp_dir().join("cryptomator_integrity_test");
        let _ = fs::remove_dir_all(&test_dir);

        create_vault(&test_dir, "verify_pw", None).unwrap();
        let key = unlock_vault(&test_dir, "verify_pw", None).unwrap();

        write_file(&test_dir, &key, "valid1.txt", b"Healthy Content 1").unwrap();
        write_file(&test_dir, &key, "valid2.txt", b"Healthy Content 2").unwrap();

        // 1. Clean vault check
        let report = verify_vault_integrity(&test_dir, &key).unwrap();
        assert_eq!(report.total_checked, 2);
        assert_eq!(report.valid_files, 2);
        assert!(report.corrupted_files.is_empty());
        assert!(report.orphaned_files.is_empty());
        assert!(report.is_healthy);

        // 2. Corrupt one file's content
        let d_dir = test_dir.join("d");
        let phys_name = get_physical_filename(&key, "valid1.txt");
        let file_path = d_dir.join(&phys_name);
        let mut corrupted_bytes = fs::read(&file_path).unwrap();
        // Flip a byte in ciphertext/tag
        if let Some(b) = corrupted_bytes.last_mut() {
            *b ^= 0xFF;
        }
        fs::write(&file_path, &corrupted_bytes).unwrap();

        // 3. Add an orphaned non-c9r file
        fs::write(d_dir.join("random_file.tmp"), b"junk").unwrap();

        // 4. Verify again - should detect corruption and orphaned file
        let report2 = verify_vault_integrity(&test_dir, &key).unwrap();
        assert_eq!(report2.total_checked, 2);
        assert_eq!(report2.valid_files, 1);
        assert_eq!(report2.corrupted_files.len(), 1);
        assert_eq!(report2.orphaned_files.len(), 1);
        assert!(!report2.is_healthy);

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_masterkey_backup_and_recovery() {
        let test_dir = std::env::temp_dir().join("cryptomator_masterkey_backup_test");
        let _ = fs::remove_dir_all(&test_dir);

        // 1. Create vault
        create_vault(&test_dir, "bkup_secret", None).unwrap();
        let primary = test_dir.join("masterkey.json");
        let backup = test_dir.join("masterkey.json.bkup");

        assert!(primary.exists(), "masterkey.json must exist");
        assert!(backup.exists(), "masterkey.json.bkup must exist");

        // 2. Write file
        let key = unlock_vault(&test_dir, "bkup_secret", None).unwrap();
        write_file(&test_dir, &key, "secrets/doc.txt", b"Safe data").unwrap();

        // 3. Delete primary masterkey.json
        fs::remove_file(&primary).unwrap();
        assert!(!primary.exists());

        // 4. Unlock must succeed via backup
        let key_from_bkup = unlock_vault(&test_dir, "bkup_secret", None).unwrap();
        assert_eq!(key, key_from_bkup);
        let content = read_file(&test_dir, &key_from_bkup, "secrets/doc.txt").unwrap();
        assert_eq!(content, b"Safe data");

        // 5. Corrupt primary masterkey.json with invalid JSON
        fs::write(&primary, b"{ this is corrupt json").unwrap();

        // 6. Unlock must still succeed by falling back to valid backup
        let key_recovered = unlock_vault(&test_dir, "bkup_secret", None).unwrap();
        assert_eq!(key, key_recovered);

        // 7. Change password
        change_vault_password(&test_dir, "bkup_secret", None, "new_secret_pwd", None).unwrap();
        let key_new = unlock_vault(&test_dir, "new_secret_pwd", None).unwrap();
        assert_eq!(key, key_new);

        let _ = fs::remove_dir_all(&test_dir);
    }
}
