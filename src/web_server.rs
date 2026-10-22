use axum::{
    Json, Router,
    body::Body,
    extract::{Multipart, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{Html, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

use crate::vault;
use crate::{SavedVault, load_saved_vaults, save_saved_vaults};

#[derive(Default)]
pub struct AppState {
    pub unlocked_vaults: RwLock<HashMap<String, [u8; 32]>>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
}

#[derive(Serialize)]
pub struct VaultInfo {
    pub name: String,
    pub path: String,
    pub is_unlocked: bool,
}

#[derive(Serialize, Deserialize)]
pub struct AddVaultRequest {
    pub name: String,
    pub path: String,
    pub password: Option<String>,
    pub create_new: bool,
}

#[derive(Serialize, Deserialize)]
pub struct UnlockVaultRequest {
    pub path: String,
    pub password: String,
    pub keyfile: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct LockVaultRequest {
    pub path: String,
}

#[derive(Deserialize)]
pub struct FilesQuery {
    pub path: String,
}

#[derive(Serialize)]
pub struct FilesResponse {
    pub files: Vec<String>,
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    pub vault_path: String,
    pub file_path: String,
}

#[derive(Deserialize)]
pub struct DeleteFileRequest {
    pub vault_path: String,
    pub file_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct RemoveVaultRequest {
    pub path: String,
}

#[derive(Serialize, Deserialize)]
pub struct MkdirRequest {
    pub vault_path: String,
    pub folder_path: String,
}

#[derive(Serialize)]
pub struct GenericSuccessResponse {
    pub success: bool,
    pub message: String,
}

async fn index_handler() -> Html<&'static str> {
    const HTML: &str = include_str!("web/index.html");
    Html(HTML)
}

async fn status_handler() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn list_vaults_handler(State(state): State<Arc<AppState>>) -> Json<Vec<VaultInfo>> {
    let saved = load_saved_vaults();
    let unlocked = state.unlocked_vaults.read().await;
    let list = saved
        .into_iter()
        .map(|v| {
            let is_unlocked = unlocked.contains_key(&v.path);
            VaultInfo {
                name: v.name,
                path: v.path,
                is_unlocked,
            }
        })
        .collect();
    Json(list)
}

async fn create_or_register_vault_handler(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<AddVaultRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let vault_path = PathBuf::from(&payload.path);
    if payload.create_new {
        let password = payload.password.as_deref().unwrap_or("");
        if password.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Password required for new vault".into(),
                }),
            ));
        }
        vault::create_vault(&vault_path, password, None).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e }),
            )
        })?;
    } else if !vault_path.join("masterkey.json").exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "No masterkey.json found at specified path".into(),
            }),
        ));
    }

    let mut saved = load_saved_vaults();
    if !saved.iter().any(|v| v.path == payload.path) {
        saved.push(SavedVault {
            name: payload.name,
            path: payload.path,
        });
        save_saved_vaults(&saved);
    }

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "Vault added successfully".into(),
    }))
}

async fn unlock_vault_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UnlockVaultRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let vault_path = Path::new(&payload.path);
    let keyfile_path = payload.keyfile.as_deref().map(Path::new);

    let master_key = vault::unlock_vault(vault_path, &payload.password, keyfile_path)
        .map_err(|e| (StatusCode::UNAUTHORIZED, Json(ErrorResponse { error: e })))?;

    let mut unlocked = state.unlocked_vaults.write().await;
    unlocked.insert(payload.path, master_key);

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "Vault unlocked successfully".into(),
    }))
}

async fn lock_vault_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LockVaultRequest>,
) -> Json<GenericSuccessResponse> {
    let mut unlocked = state.unlocked_vaults.write().await;
    unlocked.remove(&payload.path);
    Json(GenericSuccessResponse {
        success: true,
        message: "Vault locked successfully".into(),
    })
}

async fn list_files_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<FilesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let unlocked = state.unlocked_vaults.read().await;
    let master_key = unlocked.get(&query.path).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.path);
    let files = vault::list_files(vault_path, master_key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(FilesResponse { files }))
}

async fn download_file_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let unlocked = state.unlocked_vaults.read().await;
    let master_key = unlocked.get(&query.vault_path).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.vault_path);
    let content = vault::read_file(vault_path, master_key, &query.file_path)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: e })))?;

    let filename = Path::new(&query.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");

    let mime_type = mime_guess::from_path(&query.file_path)
        .first_or_octet_stream()
        .to_string();

    let disposition = format!("attachment; filename=\"{filename}\"");

    let mut response = Response::new(Body::from(content));
    let headers = response.headers_mut();
    if let Ok(mime_val) = HeaderValue::from_str(&mime_type) {
        headers.insert(header::CONTENT_TYPE, mime_val);
    }
    if let Ok(disp_val) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, disp_val);
    }

    Ok(response)
}

async fn upload_file_handler(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut vault_path = String::new();
    let mut folder = String::new();
    let mut filename = String::new();
    let mut file_bytes = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "vault_path" {
            if let Ok(text) = field.text().await {
                vault_path = text;
            }
        } else if name == "folder" {
            if let Ok(text) = field.text().await {
                folder = text;
            }
        } else if name == "file" {
            if let Some(fname) = field.file_name() {
                filename = fname.to_string();
            }
            if let Ok(bytes) = field.bytes().await {
                file_bytes = bytes.to_vec();
            }
        }
    }

    if vault_path.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing vault_path".into(),
            }),
        ));
    }
    if filename.is_empty() {
        filename = "uploaded_file".to_string();
    }

    let unlocked = state.unlocked_vaults.read().await;
    let master_key = unlocked.get(&vault_path).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked".into(),
            }),
        )
    })?;

    let relative_path = if folder.trim().is_empty() {
        filename
    } else {
        format!("{}/{}", folder.trim_matches('/'), filename)
    };

    let vault_dir = Path::new(&vault_path);
    vault::write_file(vault_dir, master_key, &relative_path, &file_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: format!("File '{relative_path}' uploaded successfully"),
    }))
}

async fn delete_file_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeleteFileRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let unlocked = state.unlocked_vaults.read().await;
    let master_key = unlocked.get(&payload.vault_path).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked".into(),
            }),
        )
    })?;

    let vault_dir = Path::new(&payload.vault_path);
    vault::delete_file(vault_dir, master_key, &payload.file_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "File deleted successfully".into(),
    }))
}

async fn preview_file_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let unlocked = state.unlocked_vaults.read().await;
    let master_key = unlocked.get(&query.vault_path).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.vault_path);
    let content = vault::read_file(vault_path, master_key, &query.file_path)
        .map_err(|e| (StatusCode::NOT_FOUND, Json(ErrorResponse { error: e })))?;

    let filename = Path::new(&query.file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("preview");

    let mime_type = mime_guess::from_path(&query.file_path)
        .first_or_octet_stream()
        .to_string();

    let disposition = format!("inline; filename=\"{filename}\"");

    let mut response = Response::new(Body::from(content));
    let headers = response.headers_mut();
    if let Ok(mime_val) = HeaderValue::from_str(&mime_type) {
        headers.insert(header::CONTENT_TYPE, mime_val);
    }
    if let Ok(disp_val) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, disp_val);
    }

    Ok(response)
}

async fn remove_vault_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RemoveVaultRequest>,
) -> Json<GenericSuccessResponse> {
    let mut unlocked = state.unlocked_vaults.write().await;
    unlocked.remove(&payload.path);

    let mut saved = load_saved_vaults();
    saved.retain(|v| v.path != payload.path);
    save_saved_vaults(&saved);

    Json(GenericSuccessResponse {
        success: true,
        message: "Vault removed from list".into(),
    })
}

async fn mkdir_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<MkdirRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let unlocked = state.unlocked_vaults.read().await;
    let master_key = unlocked.get(&payload.vault_path).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked".into(),
            }),
        )
    })?;

    let trimmed = payload.folder_path.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid folder path".into(),
            }),
        ));
    }

    let keep_file = format!("{trimmed}/.keep");
    let vault_dir = Path::new(&payload.vault_path);
    vault::write_file(vault_dir, master_key, &keep_file, b"").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: format!("Folder '{trimmed}' created"),
    }))
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/status", get(status_handler))
        .route(
            "/api/vaults",
            get(list_vaults_handler).post(create_or_register_vault_handler),
        )
        .route("/api/vaults/unlock", post(unlock_vault_handler))
        .route("/api/vaults/lock", post(lock_vault_handler))
        .route("/api/vaults/remove", post(remove_vault_handler))
        .route("/api/vaults/files", get(list_files_handler))
        .route("/api/vaults/download", get(download_file_handler))
        .route("/api/vaults/preview", get(preview_file_handler))
        .route("/api/vaults/upload", post(upload_file_handler))
        .route("/api/vaults/mkdir", post(mkdir_handler))
        .route("/api/vaults/delete", post(delete_file_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn run_server(host: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = tx.send(());
    });
    run_server_with_shutdown(host, port, rx).await
}

pub async fn run_server_with_shutdown(
    host: &str,
    port: u16,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(AppState::default());
    let app = build_router(state);

    let addr_str = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr_str).await?;
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║             Cryptomator RS - Web Control Server               ║");
    println!("╠═══════════════════════════════════════════════════════════════╣");
    println!("║ Web-Dashboard: http://{:<39}  ║", addr_str);
    println!("║ Status:        Online (Syncthing-style headless daemon)       ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!("Press Ctrl+C to stop the daemon.");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_web_status_endpoint() {
        let state = Arc::new(AppState::default());
        let router = build_router(state);

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_web_index_html() {
        let state = Arc::new(AppState::default());
        let router = build_router(state);

        let response = router
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_web_api_full_vault_lifecycle() {
        let temp_dir = std::env::temp_dir().join("cryptomator_web_test_vault");
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
        let original_saved = load_saved_vaults();

        let state = Arc::new(AppState::default());
        let router = build_router(state);

        // 1. Create vault via API
        let create_req = AddVaultRequest {
            name: "TestWebVault".to_string(),
            path: temp_dir.to_string_lossy().to_string(),
            password: Some("webpassword123".to_string()),
            create_new: true,
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&create_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 2. Unlock vault via API
        let unlock_req = UnlockVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
            password: "webpassword123".to_string(),
            keyfile: None,
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/unlock")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&unlock_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 3. Create folder via API
        let mkdir_req = MkdirRequest {
            vault_path: temp_dir.to_string_lossy().to_string(),
            folder_path: "subfolder".to_string(),
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/mkdir")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&mkdir_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 4. List files (should contain subfolder/.keep)
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/vaults/files?path={}",
                        temp_dir.to_string_lossy()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 5. Lock vault via API
        let lock_req = LockVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/lock")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&lock_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 6. List files after lock (should return 401 Unauthorized)
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/api/vaults/files?path={}",
                        temp_dir.to_string_lossy()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 7. Remove vault from list via API
        let remove_req = RemoveVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/remove")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&remove_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        save_saved_vaults(&original_saved);
    }
}
