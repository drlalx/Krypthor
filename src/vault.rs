use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use scrypt::{Params, scrypt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

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
    println!("[DEBUG] create_vault: Serializing masterkey.json content...");
    let key_file_content = serde_json::to_string_pretty(&key_file)
        .map_err(|e| format!("serde_json::to_string_pretty failed: {}", e))?;

    println!(
        "[DEBUG] create_vault: Writing masterkey.json at {:?}",
        key_file_path
    );
    fs::write(&key_file_path, key_file_content.as_bytes())
        .map_err(|e| format!("fs::write(masterkey.json) failed: {}", e))?;

    // Create an empty vault config
    let vault_config_path = vault_dir.join("vault.json");
    println!(
        "[DEBUG] create_vault: Writing vault.json at {:?}",
        vault_config_path
    );
    fs::write(&vault_config_path, b"{\"version\": 8}")
        .map_err(|e| format!("fs::write(vault.json) failed: {}", e))?;

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
    let key_file_content = serde_json::to_string_pretty(&key_file)
        .map_err(|e| format!("serde_json::to_string_pretty failed: {}", e))?;

    fs::write(&key_file_path, key_file_content.as_bytes())
        .map_err(|e| format!("fs::write(masterkey.json) failed: {}", e))?;

    Ok(())
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
    let key_file_path = vault_dir.join("masterkey.json");
    if !key_file_path.exists() {
        return Err("masterkey.json not found".to_string());
    }

    let mut file = File::open(key_file_path).map_err(|e| e.to_string())?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| e.to_string())?;

    let key_file: MasterKeyFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse masterkey.json: {}", e))?;

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
    let phys_path = vault_dir.join("d").join(phys_name);

    let payload = encrypt_file_payload(master_key, relative_path, content)?;

    fs::write(phys_path, &payload).map_err(|e| e.to_string())?;

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

pub fn list_files(vault_dir: &Path, master_key: &[u8; 32]) -> Result<Vec<String>, String> {
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
                && let Ok((rel_path, _)) = decrypt_file_payload(master_key, &payload)
            {
                files.push(rel_path);
            }
        }
    }

    // Sort files alphabetically
    files.sort();
    Ok(files)
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

        let _ = fs::remove_dir_all(&test_dir);
    }
}
