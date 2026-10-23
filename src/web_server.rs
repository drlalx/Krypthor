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

pub struct UnlockedVaultInfo {
    pub master_key: [u8; 32],
    pub last_activity: std::time::Instant,
}

#[derive(Default)]
pub struct AppState {
    pub unlocked_vaults: RwLock<HashMap<String, UnlockedVaultInfo>>,
}

impl AppState {
    pub async fn get_master_key(&self, vault_path: &str) -> Option<[u8; 32]> {
        let mut unlocked = self.unlocked_vaults.write().await;
        if let Some(info) = unlocked.get_mut(vault_path) {
            // Auto-lock after 30 minutes (1800s) of inactivity
            if info.last_activity.elapsed().as_secs() > 1800 {
                unlocked.remove(vault_path);
                None
            } else {
                info.last_activity = std::time::Instant::now();
                Some(info.master_key)
            }
        } else {
            None
        }
    }
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
    pub items: Vec<vault::FileItem>,
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
pub struct RenameVaultRequest {
    pub path: String,
    pub new_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct MkdirRequest {
    pub vault_path: String,
    pub folder_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct RenameFileRequest {
    pub vault_path: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub vault_path: String,
    pub old_password: String,
    pub new_password: String,
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryKeyRequest {
    pub vault_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct RecoverVaultRequest {
    pub path: String,
    pub recovery_key: String,
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryKeyResponse {
    pub recovery_key: String,
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
    let mut unlocked = state.unlocked_vaults.write().await;
    // Evict expired vaults (30 minutes inactivity)
    unlocked.retain(|_, info| info.last_activity.elapsed().as_secs() <= 1800);

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
    unlocked.insert(
        payload.path,
        UnlockedVaultInfo {
            master_key,
            last_activity: std::time::Instant::now(),
        },
    );

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
    let master_key = state.get_master_key(&query.path).await.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked or session expired".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.path);
    let items = vault::list_files_with_details(vault_path, &master_key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    let files = items.iter().map(|item| item.path.clone()).collect();
    Ok(Json(FilesResponse { files, items }))
}

async fn vault_stats_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<vault::VaultStats>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state.get_master_key(&query.path).await.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked or session expired".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.path);
    let stats = vault::get_vault_stats(vault_path, &master_key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(stats))
}

async fn verify_vault_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<vault::IntegrityReport>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state.get_master_key(&query.path).await.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked or session expired".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.path);
    let report = vault::verify_vault_integrity(vault_path, &master_key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(report))
}

async fn download_file_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state
        .get_master_key(&query.vault_path)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Vault is locked or session expired".into(),
                }),
            )
        })?;

    let vault_path = Path::new(&query.vault_path);
    let content = vault::read_file(vault_path, &master_key, &query.file_path)
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

async fn export_zip_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FilesQuery>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state.get_master_key(&query.path).await.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked or session expired".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.path);
    let zip_bytes = vault::export_vault_zip(vault_path, &master_key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    let vault_name = vault_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("vault");
    let disposition = format!("attachment; filename=\"{vault_name}_export.zip\"");

    let mut response = Response::new(Body::from(zip_bytes));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
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

    let master_key = state.get_master_key(&vault_path).await.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked or session expired".into(),
            }),
        )
    })?;

    let relative_path = if folder.trim().is_empty() {
        filename
    } else {
        format!("{}/{}", folder.trim_matches('/'), filename)
    };

    let vault_dir = Path::new(&vault_path);
    vault::write_file(vault_dir, &master_key, &relative_path, &file_bytes).map_err(|e| {
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
    let master_key = state
        .get_master_key(&payload.vault_path)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Vault is locked or session expired".into(),
                }),
            )
        })?;

    let vault_dir = Path::new(&payload.vault_path);
    vault::delete_file(vault_dir, &master_key, &payload.file_path).map_err(|e| {
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
    let master_key = state
        .get_master_key(&query.vault_path)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Vault is locked or session expired".into(),
                }),
            )
        })?;

    let vault_path = Path::new(&query.vault_path);
    let content = vault::read_file(vault_path, &master_key, &query.file_path)
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

async fn rename_vault_handler(
    Json(payload): Json<RenameVaultRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let trimmed = payload.new_name.trim();
    if trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Vault name cannot be empty".into(),
            }),
        ));
    }

    let mut saved = load_saved_vaults();
    let mut found = false;
    for v in &mut saved {
        if v.path == payload.path {
            v.name = trimmed.to_string();
            found = true;
            break;
        }
    }

    if !found {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Vault not found in registered list".into(),
            }),
        ));
    }

    save_saved_vaults(&saved);

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: format!("Vault renamed to '{trimmed}'"),
    }))
}

async fn mkdir_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<MkdirRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state
        .get_master_key(&payload.vault_path)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Vault is locked or session expired".into(),
                }),
            )
        })?;

    let trimmed = payload.folder_path.trim().trim_matches('/');
    let vault_dir = Path::new(&payload.vault_path);
    vault::create_dir(vault_dir, &master_key, trimmed).map_err(|e| {
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

async fn rename_file_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RenameFileRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state
        .get_master_key(&payload.vault_path)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Vault is locked or session expired".into(),
                }),
            )
        })?;

    let vault_dir = Path::new(&payload.vault_path);
    let files = vault::list_files(vault_dir, &master_key).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    let is_dir = files
        .iter()
        .any(|f| f.starts_with(&format!("{}/", payload.old_path.trim_matches('/'))));

    if is_dir {
        vault::rename_dir(vault_dir, &master_key, &payload.old_path, &payload.new_path).map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: e }),
                )
            },
        )?;
    } else {
        vault::rename_file(vault_dir, &master_key, &payload.old_path, &payload.new_path).map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: e }),
                )
            },
        )?;
    }

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "Renamed successfully".into(),
    }))
}

async fn change_password_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let vault_dir = Path::new(&payload.vault_path);
    vault::change_vault_password(
        vault_dir,
        &payload.old_password,
        None,
        &payload.new_password,
        None,
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })))?;

    // Relock after password change for security
    let mut unlocked = state.unlocked_vaults.write().await;
    unlocked.remove(&payload.vault_path);

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "Password changed successfully. Please unlock again with the new password.".into(),
    }))
}

async fn recovery_key_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecoveryKeyRequest>,
) -> Result<Json<RecoveryKeyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state
        .get_master_key(&payload.vault_path)
        .await
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Vault is locked or session expired".into(),
                }),
            )
        })?;

    let recovery_key = vault::export_recovery_key(&master_key);
    Ok(Json(RecoveryKeyResponse { recovery_key }))
}

async fn recover_vault_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RecoverVaultRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = vault::unlock_vault_with_recovery_key(&payload.recovery_key)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })))?;

    let vault_dir = Path::new(&payload.path);
    if let Err(e) = vault::list_files(vault_dir, &master_key) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: format!("Recovery key failed to decrypt vault: {e}"),
            }),
        ));
    }

    let mut unlocked = state.unlocked_vaults.write().await;
    unlocked.insert(
        payload.path,
        UnlockedVaultInfo {
            master_key,
            last_activity: std::time::Instant::now(),
        },
    );

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "Vault recovered and unlocked successfully".into(),
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
        .route("/api/vaults/recover", post(recover_vault_handler))
        .route("/api/vaults/lock", post(lock_vault_handler))
        .route("/api/vaults/remove", post(remove_vault_handler))
        .route("/api/vaults/rename_vault", post(rename_vault_handler))
        .route("/api/vaults/files", get(list_files_handler))
        .route("/api/vaults/stats", get(vault_stats_handler))
        .route("/api/vaults/verify", get(verify_vault_handler))
        .route("/api/vaults/download", get(download_file_handler))
        .route("/api/vaults/export_zip", get(export_zip_handler))
        .route("/api/vaults/preview", get(preview_file_handler))
        .route("/api/vaults/upload", post(upload_file_handler))
        .route("/api/vaults/mkdir", post(mkdir_handler))
        .route("/api/vaults/rename", post(rename_file_handler))
        .route("/api/vaults/delete", post(delete_file_handler))
        .route("/api/vaults/change_password", post(change_password_handler))
        .route("/api/vaults/recovery_key", post(recovery_key_handler))
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

    static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
        let _guard = TEST_LOCK.lock().await;
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

    #[tokio::test]
    async fn test_web_api_change_password_and_recovery() {
        let _guard = TEST_LOCK.lock().await;
        let temp_dir = std::env::temp_dir().join("cryptomator_web_test_pw_change");
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
        let original_saved = load_saved_vaults();

        let state = Arc::new(AppState::default());
        let router = build_router(state);

        // 1. Create vault
        let create_req = AddVaultRequest {
            name: "PwTestVault".to_string(),
            path: temp_dir.to_string_lossy().to_string(),
            password: Some("first_password".to_string()),
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

        // 2. Unlock vault
        let unlock_req = UnlockVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
            password: "first_password".to_string(),
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

        // 3. Export recovery key
        let rec_req = RecoveryKeyRequest {
            vault_path: temp_dir.to_string_lossy().to_string(),
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/recovery_key")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&rec_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let rec_body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let rec_data: RecoveryKeyResponse = serde_json::from_slice(&rec_body).unwrap();
        let recovery_key = rec_data.recovery_key;
        assert!(recovery_key.contains('-'));

        // 4. Change password
        let chg_req = ChangePasswordRequest {
            vault_path: temp_dir.to_string_lossy().to_string(),
            old_password: "first_password".to_string(),
            new_password: "second_password".to_string(),
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/change_password")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&chg_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 5. Old password now fails unlock
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
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 6. Recover vault using recovery key
        let recover_req = RecoverVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
            recovery_key,
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/recover")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&recover_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 7. Lock vault again
        let lock_req = LockVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
        };
        let _ = router
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

        // 8. New password unlocks successfully
        let unlock_new_req = UnlockVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
            password: "second_password".to_string(),
            keyfile: None,
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/unlock")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&unlock_new_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 9. Rename vault via API
        let rename_v_req = RenameVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
            new_name: "RenamedVault".to_string(),
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/rename_vault")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&rename_v_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 10. Export ZIP via API
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/vaults/export_zip?path={}",
                        temp_dir.to_string_lossy()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers()
                .get(header::CONTENT_TYPE)
                .unwrap()
                .to_str()
                .unwrap(),
            "application/zip"
        );
        let zip_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(!zip_bytes.is_empty());

        // 11. Verify vault integrity via API
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/vaults/verify?path={}",
                        temp_dir.to_string_lossy()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let verify_body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let report: vault::IntegrityReport = serde_json::from_slice(&verify_body).unwrap();
        assert!(report.is_healthy);
        assert_eq!(report.corrupted_files.len(), 0);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        save_saved_vaults(&original_saved);
    }
}
