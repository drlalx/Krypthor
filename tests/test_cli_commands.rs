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

    // Cleanup
    let _ = fs::remove_dir_all(&test_dir);
}
