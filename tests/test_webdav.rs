#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(feature = "web")]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use cryptomator_rs::web_server::{AppState, build_router};
    use cryptomator_rs::webdav_fs::create_webdav_handler;
    use cryptomator_rs::{SavedVault, vault};
    use std::fs;
    use std::sync::Arc;
    use std::time::Instant;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_webdav_operations_in_memory() {
        let temp_dir =
            std::env::temp_dir().join(format!("cryptomator_webdav_test_{}", std::process::id()));
        if temp_dir.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        let vault_dir = temp_dir.join("webdav_vault");
        let password = "webdav_secret_password";

        // 1. Create and unlock a vault
        vault::create_vault(&vault_dir, password, None).unwrap();
        let master_key = vault::unlock_vault(&vault_dir, password, None).unwrap();

        // 2. Populate an initial file
        vault::write_file(&vault_dir, &master_key, "hello.txt", b"Hello from WebDAV!").unwrap();

        // 3. Create WebDAV handler
        let dav_handler = Arc::new(create_webdav_handler(vault_dir.clone(), master_key, None));

        let app = axum::Router::new().fallback(move |req: axum::extract::Request| {
            let handler = dav_handler.clone();
            async move {
                let (parts, body) = handler.handle(req).await.into_parts();
                axum::response::Response::from_parts(parts, axum::body::Body::new(body))
            }
        });

        // Test OPTIONS /
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let dav_header = resp.headers().get("DAV").and_then(|h| h.to_str().ok());
        assert!(
            dav_header.is_some(),
            "DAV header should be present in OPTIONS"
        );

        // Test GET /hello.txt
        let req = Request::builder()
            .method("GET")
            .uri("/hello.txt")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body_bytes[..], b"Hello from WebDAV!");

        // Test PROPFIND / (should be Multi-Status 207)
        let req = Request::builder()
            .method("PROPFIND")
            .uri("/")
            .header("Depth", "1")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::MULTI_STATUS);
        let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let xml_str = String::from_utf8_lossy(&body_bytes);
        assert!(xml_str.contains("hello.txt"));

        // Test PUT /notes.txt (writing a new file via WebDAV)
        let req = Request::builder()
            .method("PUT")
            .uri("/notes.txt")
            .body(Body::from("Uploaded document content through WebDAV PUT"))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert!(resp.status().is_success() || resp.status() == StatusCode::CREATED);

        // Verify the file was encrypted in vault and can be read back
        let read_back = vault::read_file(&vault_dir, &master_key, "notes.txt").unwrap();
        assert_eq!(
            &read_back[..],
            b"Uploaded document content through WebDAV PUT"
        );

        // Test MKCOL /documents (creating a directory via WebDAV)
        let req = Request::builder()
            .method("MKCOL")
            .uri("/documents")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert!(resp.status().is_success() || resp.status() == StatusCode::CREATED);

        // Test PUT into subdirectory: /documents/report.txt
        let req = Request::builder()
            .method("PUT")
            .uri("/documents/report.txt")
            .body(Body::from("Subdirectory file content"))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert!(resp.status().is_success() || resp.status() == StatusCode::CREATED);

        let sub_read = vault::read_file(&vault_dir, &master_key, "documents/report.txt").unwrap();
        assert_eq!(&sub_read[..], b"Subdirectory file content");

        // Test DELETE /notes.txt
        let req = Request::builder()
            .method("DELETE")
            .uri("/notes.txt")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert!(resp.status().is_success() || resp.status() == StatusCode::NO_CONTENT);
        assert!(vault::read_file(&vault_dir, &master_key, "notes.txt").is_err());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_daemon_webdav_routing() {
        let temp_dir = std::env::temp_dir().join(format!(
            "cryptomator_daemon_webdav_test_{}",
            std::process::id()
        ));
        if temp_dir.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        let vault_dir = temp_dir.join("daemon_vault");
        let password = "daemon_password";

        vault::create_vault(&vault_dir, password, None).unwrap();
        let master_key = vault::unlock_vault(&vault_dir, password, None).unwrap();
        vault::write_file(&vault_dir, &master_key, "daemon_test.txt", b"Daemon WebDAV").unwrap();

        let state = Arc::new(AppState::default());
        let app = build_router(Arc::clone(&state));

        let vault_path_str = vault_dir.to_str().unwrap().to_string();

        // Register vault in configuration
        cryptomator_rs::save_saved_vaults(&[SavedVault {
            name: "MyDaemonVault".to_string(),
            path: vault_path_str.clone(),
        }]);

        // 1. Try accessing WebDAV when vault is locked -> should return 401 Unauthorized
        let req = Request::builder()
            .method("GET")
            .uri("/webdav/MyDaemonVault/daemon_test.txt")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // 2. Unlock vault in state
        {
            let mut unlocked = state.unlocked_vaults.write().await;
            unlocked.insert(
                vault_path_str.clone(),
                cryptomator_rs::web_server::UnlockedVaultInfo {
                    master_key,
                    last_activity: Instant::now(),
                    timeout_seconds: 3600,
                },
            );
        }

        // 3. Access WebDAV now -> should succeed with 200 OK
        let req = Request::builder()
            .method("GET")
            .uri("/webdav/MyDaemonVault/daemon_test.txt")
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..], b"Daemon WebDAV");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
