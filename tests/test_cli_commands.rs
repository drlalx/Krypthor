#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::process::Command;

#[test]
fn test_cli_vault_workflow() {
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
            &import_src.to_string_lossy().to_string(),
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
            &export_dst.to_string_lossy().to_string(),
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

    let _ = fs::remove_file(&tmp_src);
    let _ = fs::remove_dir_all(&import_src);
    let _ = fs::remove_dir_all(&export_dst);

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}
