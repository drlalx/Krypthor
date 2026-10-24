#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::process::Command;
use std::sync::Mutex;

static CLI_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_cli_vault_workflow() {
    let _lock = CLI_TEST_LOCK.lock().unwrap();
    let bin_path = env!("CARGO_BIN_EXE_cryptomator_rs");
    let test_dir = std::env::temp_dir().join("cryptomator_cli_integration_test");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(&test_dir);
    }

    let vault_path = test_dir.to_string_lossy().to_string();

    // 1. Create vault via CLI
    let output = Command::new(bin_path)
        .args([
            "create",
            "--name",
            "CliTestVault",
            "--path",
            &vault_path,
            "--password",
            "initial_secret",
        ])
        .output()
        .expect("Failed to execute create command");
    assert!(
        output.status.success(),
        "Vault creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 2. Export recovery key via CLI
    let output = Command::new(bin_path)
        .args([
            "recovery-key",
            "--path",
            &vault_path,
            "--password",
            "initial_secret",
        ])
        .output()
        .expect("Failed to execute recovery-key command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Vault Recovery Key:"));

    // Extract recovery key from stdout
    let mut recovery_key = String::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.contains('-') && trimmed.len() >= 64 {
            recovery_key = trimmed.to_string();
            break;
        }
    }
    assert!(
        !recovery_key.is_empty(),
        "Failed to extract recovery key from output: {stdout}"
    );

    // 3. Change password via CLI
    let output = Command::new(bin_path)
        .args([
            "change-password",
            "--path",
            &vault_path,
            "--old-password",
            "initial_secret",
            "--new-password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute change-password command");
    assert!(output.status.success());

    // 4. Old password should now fail to export recovery key
    let output = Command::new(bin_path)
        .args([
            "recovery-key",
            "--path",
            &vault_path,
            "--password",
            "initial_secret",
        ])
        .output()
        .expect("Failed to execute recovery-key command with old password");
    assert!(!output.status.success());

    // 5. New password succeeds
    let output = Command::new(bin_path)
        .args([
            "recovery-key",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute recovery-key command with new password");
    assert!(output.status.success());

    // 6. Recover vault using recovery key
    let output = Command::new(bin_path)
        .args(["recover", "--path", &vault_path, "--key", &recovery_key])
        .output()
        .expect("Failed to execute recover command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Vault successfully decrypted using recovery key!"));

    // 7. Store file via CLI (put)
    let tmp_src = std::env::temp_dir().join("cryptomator_cli_sample.txt");
    fs::write(&tmp_src, b"Confidential financial data").unwrap();

    let output = Command::new(bin_path)
        .args([
            "put",
            "--path",
            &vault_path,
            "--source",
            &tmp_src.to_string_lossy(),
            "--target",
            "finances/report.txt",
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute put command");
    assert!(output.status.success());

    // 8. List files via CLI (ls)
    let output = Command::new(bin_path)
        .args(["ls", "--path", &vault_path, "--password", "updated_secret"])
        .output()
        .expect("Failed to execute ls command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("finances/report.txt"));

    // 8b. List files detailed (ls -l)
    let output = Command::new(bin_path)
        .args([
            "ls",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
            "-l",
        ])
        .output()
        .expect("Failed to execute ls -l command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("finances/report.txt"));
    assert!(stdout.contains("202")); // year 202x

    // 8c. List files filtered (ls --pattern)
    let output = Command::new(bin_path)
        .args([
            "ls",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
            "--pattern",
            "report",
        ])
        .output()
        .expect("Failed to execute ls --pattern");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("finances/report.txt"));

    // 8d. Find files via CLI (find)
    let output = Command::new(bin_path)
        .args([
            "find",
            "--path",
            &vault_path,
            "-q",
            "report",
            "--password",
            "updated_secret",
            "-l",
        ])
        .output()
        .expect("Failed to execute find command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("finances/report.txt"));

    let output = Command::new(bin_path)
        .args([
            "find",
            "--path",
            &vault_path,
            "-q",
            "nonexistent_xyz",
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute find command with no match");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("No files matching 'nonexistent_xyz' found."));

    // 8e. List files JSON format (ls --json)
    let output = Command::new(bin_path)
        .args([
            "ls",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
            "--json",
        ])
        .output()
        .expect("Failed to execute ls --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"path\": \"finances/report.txt\""));

    // 9. Read file content via CLI (cat)
    let output = Command::new(bin_path)
        .args([
            "cat",
            "--path",
            &vault_path,
            "--file",
            "finances/report.txt",
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute cat command");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"Confidential financial data");

    // 10. Check stats via CLI
    let output = Command::new(bin_path)
        .args([
            "stats",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute stats command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Decrypted Files: 1"));

    // 10b. Check stats JSON format
    let output = Command::new(bin_path)
        .args([
            "stats",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
            "--json",
        ])
        .output()
        .expect("Failed to execute stats --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"total_files\": 1"));

    // 11. Rename/move file via CLI (mv)
    let output = Command::new(bin_path)
        .args([
            "mv",
            "--path",
            &vault_path,
            "--old-path",
            "finances/report.txt",
            "--new-path",
            "archive/old_report.txt",
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute mv command");
    assert!(output.status.success());

    // 12. Delete file via CLI (rm)
    let output = Command::new(bin_path)
        .args([
            "rm",
            "--path",
            &vault_path,
            "--file",
            "archive/old_report.txt",
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute rm command");
    assert!(output.status.success());

    // 13. ls now shows empty
    let output = Command::new(bin_path)
        .args(["ls", "--path", &vault_path, "--password", "updated_secret"])
        .output()
        .expect("Failed to execute ls command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Vault is empty."));

    // 14. Create folder via CLI (mkdir)
    let output = Command::new(bin_path)
        .args([
            "mkdir",
            "--path",
            &vault_path,
            "--folder",
            "documents/legal",
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute mkdir command");
    assert!(output.status.success());

    // 15. Import directory via CLI (import)
    let import_src = std::env::temp_dir().join("cryptomator_cli_import_source");
    let _ = fs::remove_dir_all(&import_src);
    fs::create_dir_all(import_src.join("sub")).unwrap();
    fs::write(import_src.join("a.txt"), b"File A content").unwrap();
    fs::write(import_src.join("sub/b.txt"), b"File B content").unwrap();

    let output = Command::new(bin_path)
        .args([
            "import",
            "--path",
            &vault_path,
            "--source",
            &import_src.to_string_lossy(),
            "--dest",
            "imported",
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute import command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Successfully imported 2 file(s)"));

    // 16. Export vault via CLI (export)
    let export_dst = std::env::temp_dir().join("cryptomator_cli_export_target");
    let _ = fs::remove_dir_all(&export_dst);

    let output = Command::new(bin_path)
        .args([
            "export",
            "--path",
            &vault_path,
            "--dest",
            &export_dst.to_string_lossy(),
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute export command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Successfully exported 2 decrypted file(s)"));

    assert_eq!(
        fs::read(export_dst.join("imported/a.txt")).unwrap(),
        b"File A content"
    );
    assert_eq!(
        fs::read(export_dst.join("imported/sub/b.txt")).unwrap(),
        b"File B content"
    );
    assert!(export_dst.join("documents/legal").is_dir());

    // 17. Rename vault via CLI (rename-vault)
    let output = Command::new(bin_path)
        .args([
            "rename-vault",
            "--path",
            &vault_path,
            "--new-name",
            "SuperVault",
        ])
        .output()
        .expect("Failed to execute rename-vault command");
    assert!(output.status.success());

    // 18. List vaults via CLI and verify renamed name
    let output = Command::new(bin_path)
        .args(["list"])
        .output()
        .expect("Failed to execute list command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SuperVault"));

    // 18b. List vaults via CLI in JSON format
    let output = Command::new(bin_path)
        .args(["list", "--json"])
        .output()
        .expect("Failed to execute list --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"name\": \"SuperVault\""));

    // 19. Remove vault from vaults.json via CLI (remove)
    let output = Command::new(bin_path)
        .args(["remove", "--path", &vault_path])
        .output()
        .expect("Failed to execute remove command");
    assert!(output.status.success());

    // 20. Re-register vault via CLI (add)
    let output = Command::new(bin_path)
        .args(["add", "--path", &vault_path, "--name", "ReAddedVault"])
        .output()
        .expect("Failed to execute add command");
    assert!(output.status.success());

    // 21. Verify vault integrity via CLI (verify)
    let output = Command::new(bin_path)
        .args([
            "verify",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute verify command");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Status:           HEALTHY"));
    assert!(stdout.contains("Valid Files:      3"));

    // 21b. Verify vault integrity in JSON format
    let output = Command::new(bin_path)
        .args([
            "verify",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
            "--json",
        ])
        .output()
        .expect("Failed to execute verify --json");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"is_healthy\": true"));

    // 22. Backup masterkey via CLI (backup-key)
    let custom_backup = test_dir.join("explicit_backup.json");
    let output = Command::new(bin_path)
        .args([
            "backup-key",
            "--path",
            &vault_path,
            "--out",
            &custom_backup.to_string_lossy(),
        ])
        .output()
        .expect("Failed to execute backup-key command");
    assert!(output.status.success());
    assert!(custom_backup.exists());

    // 23. Corrupt primary masterkey.json
    let primary_mk = test_dir.join("masterkey.json");
    fs::write(&primary_mk, b"broken json").unwrap();

    // 24. Restore masterkey via CLI (restore-key)
    let output = Command::new(bin_path)
        .args([
            "restore-key",
            "--path",
            &vault_path,
            "--from",
            &custom_backup.to_string_lossy(),
        ])
        .output()
        .expect("Failed to execute restore-key command");
    assert!(output.status.success());

    // 25. Verify vault again to confirm healthy restored masterkey
    let output = Command::new(bin_path)
        .args([
            "verify",
            "--path",
            &vault_path,
            "--password",
            "updated_secret",
        ])
        .output()
        .expect("Failed to execute verify command after restore");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Status:           HEALTHY"));

    // Cleanup vault from vaults.json
    let _ = Command::new(bin_path)
        .args(["remove", "--path", &vault_path])
        .output();

    let _ = fs::remove_file(&tmp_src);
    let _ = fs::remove_dir_all(&import_src);
    let _ = fs::remove_dir_all(&export_dst);

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_cli_keyfile_workflow() {
    let _lock = CLI_TEST_LOCK.lock().unwrap();
    let bin_path = env!("CARGO_BIN_EXE_cryptomator_rs");
    let test_dir = std::env::temp_dir().join("cryptomator_cli_keyfile_test");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(&test_dir);
    }
    fs::create_dir_all(&test_dir).expect("create test_dir");

    let vault_dir = test_dir.join("vault");
    let vault_path = vault_dir.to_string_lossy().to_string();
    let keyfile_1 = test_dir.join("keyfile1.bin");
    let keyfile_2 = test_dir.join("keyfile2.bin");

    fs::write(&keyfile_1, b"first-super-secret-keyfile-content-32b!").expect("write keyfile1");
    fs::write(&keyfile_2, b"second-super-secret-keyfile-content-32b!").expect("write keyfile2");

    let kf1_path = keyfile_1.to_string_lossy().to_string();
    let kf2_path = keyfile_2.to_string_lossy().to_string();

    // 1. Create vault with keyfile
    let output = Command::new(bin_path)
        .args([
            "create",
            "--name",
            "KeyfileVault",
            "--path",
            &vault_path,
            "--password",
            "mypassword123",
            "--keyfile",
            &kf1_path,
        ])
        .output()
        .expect("create vault with keyfile");
    assert!(
        output.status.success(),
        "Vault creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 2. Listing files without keyfile must fail
    let output = Command::new(bin_path)
        .args(["ls", "--path", &vault_path, "--password", "mypassword123"])
        .output()
        .expect("ls without keyfile");
    assert!(
        !output.status.success(),
        "Unlock without keyfile should have failed!"
    );

    // 3. Put a file using keyfile
    let sample_file = test_dir.join("secret_note.txt");
    fs::write(&sample_file, b"Confidential memo").expect("write sample");
    let output = Command::new(bin_path)
        .args([
            "put",
            "--path",
            &vault_path,
            "--source",
            &sample_file.to_string_lossy(),
            "--target",
            "notes/memo.txt",
            "--password",
            "mypassword123",
            "--keyfile",
            &kf1_path,
        ])
        .output()
        .expect("put with keyfile");
    assert!(
        output.status.success(),
        "Put failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 4. Cat the file using keyfile
    let output = Command::new(bin_path)
        .args([
            "cat",
            "--path",
            &vault_path,
            "--file",
            "notes/memo.txt",
            "--password",
            "mypassword123",
            "--keyfile",
            &kf1_path,
        ])
        .output()
        .expect("cat with keyfile");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"Confidential memo");

    // 5. Change password and keyfile via CLI
    let output = Command::new(bin_path)
        .args([
            "change-password",
            "--path",
            &vault_path,
            "--old-password",
            "mypassword123",
            "--old-keyfile",
            &kf1_path,
            "--new-password",
            "newpassword456",
            "--new-keyfile",
            &kf2_path,
        ])
        .output()
        .expect("change password and keyfile");
    assert!(
        output.status.success(),
        "Change password failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 6. Old keyfile + old password fails
    let output = Command::new(bin_path)
        .args([
            "stats",
            "--path",
            &vault_path,
            "--password",
            "mypassword123",
            "--keyfile",
            &kf1_path,
        ])
        .output()
        .expect("stats with old creds");
    assert!(!output.status.success());

    // 7. New keyfile + new password succeeds
    let output = Command::new(bin_path)
        .args([
            "stats",
            "--path",
            &vault_path,
            "--password",
            "newpassword456",
            "--keyfile",
            &kf2_path,
        ])
        .output()
        .expect("stats with new creds");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Decrypted Files: 1"));

    // 8. Verify vault integrity with new keyfile
    let output = Command::new(bin_path)
        .args([
            "verify",
            "--path",
            &vault_path,
            "--password",
            "newpassword456",
            "--keyfile",
            &kf2_path,
        ])
        .output()
        .expect("verify with new keyfile");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Status:           HEALTHY"));

    // Cleanup
    let _ = Command::new(bin_path)
        .args(["remove", "--path", &vault_path])
        .output();
    let _ = fs::remove_dir_all(&test_dir);
}
