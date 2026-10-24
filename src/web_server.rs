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
    pub timeout_seconds: u64,
}

#[derive(Default)]
pub struct AppState {
    pub unlocked_vaults: RwLock<HashMap<String, UnlockedVaultInfo>>,
}

impl AppState {
    pub async fn get_master_key(&self, vault_path: &str) -> Option<[u8; 32]> {
        let mut unlocked = self.unlocked_vaults.write().await;
        if let Some(info) = unlocked.get_mut(vault_path) {
            if info.timeout_seconds > 0
                && info.last_activity.elapsed().as_secs() > info.timeout_seconds
            {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_lock_remaining_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_lock_timeout_seconds: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct AddVaultRequest {
    pub name: String,
    pub path: String,
    pub password: Option<String>,
    pub create_new: bool,
    #[serde(default)]
    pub keyfile: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UnlockVaultRequest {
    pub path: String,
    pub password: String,
    #[serde(default)]
    pub keyfile: Option<String>,
    #[serde(default)]
    pub timeout_minutes: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct LockVaultRequest {
    pub path: String,
}

#[derive(Deserialize)]
pub struct FilesQuery {
    pub path: String,
}

#[derive(Deserialize)]
pub struct ExportZipQuery {
    pub path: String,
    pub subfolder: Option<String>,
}

#[derive(Deserialize)]
pub struct FindQuery {
    pub path: String,
    pub q: String,
}

#[derive(Deserialize)]
pub struct GrepQuery {
    pub path: String,
    pub q: String,
    pub case_sensitive: Option<bool>,
    pub subfolder: Option<String>,
    pub max_matches: Option<usize>,
}

#[derive(Deserialize)]
pub struct HashQuery {
    pub path: String,
    pub file: String,
}

#[derive(Deserialize)]
pub struct DiffQuery {
    pub path: String,
    pub file_a: String,
    pub file_b: String,
}

#[derive(Serialize, Deserialize)]
pub struct PruneRequest {
    pub vault_path: String,
    #[serde(default)]
    pub dry_run: bool,
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
pub struct CopyFileRequest {
    pub vault_path: String,
    pub src_path: String,
    pub dst_path: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Serialize, Deserialize)]
pub struct CopyFileResponse {
    pub success: bool,
    pub copied_count: usize,
}

#[derive(Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub vault_path: String,
    pub old_password: String,
    pub new_password: String,
    #[serde(default)]
    pub old_keyfile: Option<String>,
    #[serde(default)]
    pub new_keyfile: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryKeyRequest {
    pub vault_path: String,
}

#[derive(Serialize, Deserialize)]
pub struct BackupKeyRequest {
    pub vault_path: String,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RestoreKeyRequest {
    pub vault_path: String,
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RecoverVaultRequest {
    pub path: String,
    pub recovery_key: String,
    #[serde(default)]
    pub timeout_minutes: Option<u64>,
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
    // Evict expired vaults
    unlocked.retain(|_, info| {
        if info.timeout_seconds > 0 {
            info.last_activity.elapsed().as_secs() <= info.timeout_seconds
        } else {
            true
        }
    });

    let list = saved
        .into_iter()
        .map(|v| {
            if let Some(info) = unlocked.get(&v.path) {
                let remaining = if info.timeout_seconds > 0 {
                    let elapsed = info.last_activity.elapsed().as_secs();
                    Some(info.timeout_seconds.saturating_sub(elapsed))
                } else {
                    None
                };
                VaultInfo {
                    name: v.name,
                    path: v.path,
                    is_unlocked: true,
                    auto_lock_remaining_seconds: remaining,
                    auto_lock_timeout_seconds: Some(info.timeout_seconds),
                }
            } else {
                VaultInfo {
                    name: v.name,
                    path: v.path,
                    is_unlocked: false,
                    auto_lock_remaining_seconds: None,
                    auto_lock_timeout_seconds: None,
                }
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
        let keyfile_path = payload.keyfile.as_deref().map(Path::new);
        vault::create_vault(&vault_path, password, keyfile_path).map_err(|e| {
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

    let timeout_seconds = payload.timeout_minutes.unwrap_or(30) * 60;
    let mut unlocked = state.unlocked_vaults.write().await;
    unlocked.insert(
        payload.path,
        UnlockedVaultInfo {
            master_key,
            last_activity: std::time::Instant::now(),
            timeout_seconds,
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

async fn find_files_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FindQuery>,
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
    let items = vault::find_files(vault_path, &master_key, &query.q).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    let files = items.iter().map(|item| item.path.clone()).collect();
    Ok(Json(FilesResponse { files, items }))
}

async fn grep_files_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<GrepQuery>,
) -> Result<Json<Vec<vault::FileGrepResult>>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state.get_master_key(&query.path).await.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked or session expired".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.path);
    let results = vault::grep_vault(
        vault_path,
        &master_key,
        &query.q,
        query.case_sensitive.unwrap_or(false),
        query.subfolder.as_deref(),
        query.max_matches,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(results))
}

async fn diff_files_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<vault::DiffResult>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state.get_master_key(&query.path).await.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked or session expired".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.path);
    let diff = vault::diff_vault_files(vault_path, &master_key, &query.file_a, &query.file_b)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e }),
            )
        })?;

    Ok(Json(diff))
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
    Query(query): Query<ExportZipQuery>,
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
    let subfolder_ref = query.subfolder.as_deref().filter(|s| !s.trim().is_empty());
    let zip_bytes =
        vault::export_vault_zip(vault_path, &master_key, subfolder_ref).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e }),
            )
        })?;

    let vault_name = vault_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("vault");
    let sub_suffix = match subfolder_ref {
        Some(s) => {
            let clean = s.trim_matches('/').replace('/', "_");
            format!("_{clean}")
        }
        None => String::new(),
    };
    let disposition = format!("attachment; filename=\"{vault_name}{sub_suffix}_export.zip\"");

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
    let trimmed = payload.file_path.trim().trim_matches('/');
    let file_res = vault::delete_file(vault_dir, &master_key, trimmed);
    if file_res.is_err() {
        vault::delete_dir(vault_dir, &master_key, trimmed).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: e }),
            )
        })?;
    } else {
        let _ = vault::delete_dir(vault_dir, &master_key, trimmed);
    }

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "File or directory deleted successfully".into(),
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

async fn copy_file_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CopyFileRequest>,
) -> Result<Json<CopyFileResponse>, (StatusCode, Json<ErrorResponse>)> {
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

    let clean_src = payload.src_path.trim().trim_matches('/');
    let is_dir = files
        .iter()
        .any(|f| f.starts_with(&format!("{clean_src}/")));

    let copied_count = if is_dir {
        if !payload.recursive {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Source is a directory; recursive copy required".into(),
                }),
            ));
        }
        vault::copy_dir(vault_dir, &master_key, &payload.src_path, &payload.dst_path).map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: e }),
                )
            },
        )?
    } else {
        vault::copy_file(vault_dir, &master_key, &payload.src_path, &payload.dst_path).map_err(
            |e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: e }),
                )
            },
        )?;
        1
    };

    Ok(Json(CopyFileResponse {
        success: true,
        copied_count,
    }))
}

async fn change_password_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let vault_dir = Path::new(&payload.vault_path);
    let old_keyfile_path = payload.old_keyfile.as_deref().map(Path::new);
    let new_keyfile_path = payload.new_keyfile.as_deref().map(Path::new);
    vault::change_vault_password(
        vault_dir,
        &payload.old_password,
        old_keyfile_path,
        &payload.new_password,
        new_keyfile_path,
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

    let timeout_seconds = payload.timeout_minutes.unwrap_or(30) * 60;
    let mut unlocked = state.unlocked_vaults.write().await;
    unlocked.insert(
        payload.path,
        UnlockedVaultInfo {
            master_key,
            last_activity: std::time::Instant::now(),
            timeout_seconds,
        },
    );

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "Vault recovered and unlocked successfully".into(),
    }))
}

async fn backup_key_handler(
    Json(payload): Json<BackupKeyRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let vault_dir = Path::new(&payload.vault_path);
    let out_path = payload.out.as_deref().map(Path::new);
    let dest = vault::create_masterkey_backup(vault_dir, out_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: format!("Masterkey backup created at '{}'", dest.display()),
    }))
}

async fn restore_key_handler(
    Json(payload): Json<RestoreKeyRequest>,
) -> Result<Json<GenericSuccessResponse>, (StatusCode, Json<ErrorResponse>)> {
    let vault_dir = Path::new(&payload.vault_path);
    let from_path = payload.from.as_deref().map(Path::new);
    vault::restore_masterkey_backup(vault_dir, from_path)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e })))?;

    Ok(Json(GenericSuccessResponse {
        success: true,
        message: "Masterkey successfully restored from backup".into(),
    }))
}

async fn system_benchmark_handler() -> Json<vault::BenchmarkResult> {
    tokio::task::spawn_blocking(vault::run_benchmark)
        .await
        .map(Json)
        .unwrap_or_else(|_| Json(vault::run_benchmark()))
}

async fn system_doctor_handler() -> Json<vault::DoctorReport> {
    let saved = crate::load_saved_vaults();
    let paths: Vec<String> = saved.into_iter().map(|v| v.path).collect();
    tokio::task::spawn_blocking(move || vault::run_doctor(&paths))
        .await
        .map(Json)
        .unwrap_or_else(|_| Json(vault::run_doctor(&[])))
}

async fn hash_file_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashQuery>,
) -> Result<Json<vault::FileHashResult>, (StatusCode, Json<ErrorResponse>)> {
    let master_key = state.get_master_key(&query.path).await.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Vault is locked or session expired".into(),
            }),
        )
    })?;

    let vault_path = Path::new(&query.path);
    let result = vault::hash_file(vault_path, &master_key, &query.file).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(result))
}

async fn prune_vault_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PruneRequest>,
) -> Result<Json<vault::PruneReport>, (StatusCode, Json<ErrorResponse>)> {
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

    let vault_path = Path::new(&payload.vault_path);
    let report = vault::prune_vault(vault_path, &master_key, payload.dry_run).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
    })?;

    Ok(Json(report))
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/status", get(status_handler))
        .route("/api/system/benchmark", get(system_benchmark_handler))
        .route("/api/system/doctor", get(system_doctor_handler))
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
        .route("/api/vaults/find", get(find_files_handler))
        .route("/api/vaults/grep", get(grep_files_handler))
        .route("/api/vaults/diff", get(diff_files_handler))
        .route("/api/vaults/stats", get(vault_stats_handler))
        .route("/api/vaults/verify", get(verify_vault_handler))
        .route("/api/vaults/prune", post(prune_vault_handler))
        .route("/api/vaults/hash", get(hash_file_handler))
        .route("/api/vaults/download", get(download_file_handler))
        .route("/api/vaults/export_zip", get(export_zip_handler))
        .route("/api/vaults/preview", get(preview_file_handler))
        .route("/api/vaults/upload", post(upload_file_handler))
        .route("/api/vaults/mkdir", post(mkdir_handler))
        .route("/api/vaults/rename", post(rename_file_handler))
        .route("/api/vaults/copy", post(copy_file_handler))
        .route("/api/vaults/delete", post(delete_file_handler))
        .route("/api/vaults/change_password", post(change_password_handler))
        .route("/api/vaults/recovery_key", post(recovery_key_handler))
        .route("/api/vaults/backup_key", post(backup_key_handler))
        .route("/api/vaults/restore_key", post(restore_key_handler))
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

    // Periodic background cleanup of expired vault sessions
    let bg_state = Arc::clone(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            let mut unlocked = bg_state.unlocked_vaults.write().await;
            unlocked.retain(|_path, info| {
                if info.timeout_seconds > 0 {
                    info.last_activity.elapsed().as_secs() <= info.timeout_seconds
                } else {
                    true
                }
            });
        }
    });

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
        let router = build_router(state.clone());

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
        let router = build_router(state.clone());

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
        let router = build_router(state.clone());

        // 1. Create vault via API
        let create_req = AddVaultRequest {
            name: "TestWebVault".to_string(),
            path: temp_dir.to_string_lossy().to_string(),
            password: Some("webpassword123".to_string()),
            create_new: true,
            keyfile: None,
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
            timeout_minutes: None,
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
        let router = build_router(state.clone());

        // 1. Create vault
        let create_req = AddVaultRequest {
            name: "PwTestVault".to_string(),
            path: temp_dir.to_string_lossy().to_string(),
            password: Some("first_password".to_string()),
            create_new: true,
            keyfile: None,
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
            timeout_minutes: None,
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
            old_keyfile: None,
            new_keyfile: None,
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
            timeout_minutes: None,
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
            timeout_minutes: None,
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

        // 9b. Mkdir & Copy file via API
        let mkdir_req = MkdirRequest {
            vault_path: temp_dir.to_string_lossy().to_string(),
            folder_path: "folder".to_string(),
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

        let copy_req = CopyFileRequest {
            vault_path: temp_dir.to_string_lossy().to_string(),
            src_path: "folder/.keep".to_string(),
            dst_path: "folder/copied_keep.txt".to_string(),
            recursive: false,
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/copy")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&copy_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let copy_body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let copy_resp: CopyFileResponse = serde_json::from_slice(&copy_body).unwrap();
        assert!(copy_resp.success);
        assert_eq!(copy_resp.copied_count, 1);

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

        // 10b. Export subfolder ZIP via API
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/vaults/export_zip?path={}&subfolder=folder",
                        temp_dir.to_string_lossy()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let sub_zip_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(!sub_zip_bytes.is_empty());

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

        // 11b. Hash file via API
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/vaults/hash?path={}&file=folder/.keep",
                        temp_dir.to_string_lossy()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let hash_body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let hash_data: vault::FileHashResult = serde_json::from_slice(&hash_body).unwrap();
        assert_eq!(hash_data.file, "folder/.keep");
        assert!(!hash_data.sha256.is_empty());

        // 11c. Prune vault via API
        let prune_req = PruneRequest {
            vault_path: temp_dir.to_string_lossy().to_string(),
            dry_run: true,
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/prune")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&prune_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let prune_body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let prune_report: vault::PruneReport = serde_json::from_slice(&prune_body).unwrap();
        assert!(prune_report.dry_run);

        // 12. System benchmark via API
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/system/benchmark")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bench_body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let bench_res: vault::BenchmarkResult = serde_json::from_slice(&bench_body).unwrap();
        assert!(bench_res.scrypt_derivations_per_sec > 0.0);
        assert!(bench_res.aes_gcm_encrypt_mb_per_sec > 0.0);

        // 13. System doctor via API
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/system/doctor")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let doc_body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let doc_res: vault::DoctorReport = serde_json::from_slice(&doc_body).unwrap();
        assert!(doc_res.checks.len() >= 3);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        save_saved_vaults(&original_saved);
    }

    #[tokio::test]
    async fn test_web_api_keyfile_lifecycle() {
        let _guard = TEST_LOCK.lock().await;
        let temp_dir = std::env::temp_dir().join("cryptomator_web_test_kf_lifecycle");
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
        std::fs::create_dir_all(&temp_dir).unwrap();
        let original_saved = load_saved_vaults();

        let vault_dir = temp_dir.join("vault");
        let kf1 = temp_dir.join("kf1.key");
        let kf2 = temp_dir.join("kf2.key");
        std::fs::write(&kf1, b"keyfile-secret-web-one-32-bytes!").unwrap();
        std::fs::write(&kf2, b"keyfile-secret-web-two-32-bytes!").unwrap();

        let kf1_str = kf1.to_string_lossy().to_string();
        let kf2_str = kf2.to_string_lossy().to_string();

        let state = Arc::new(AppState::default());
        let router = build_router(state.clone());

        // 1. Create vault with keyfile via API
        let create_req = AddVaultRequest {
            name: "KfWebVault".to_string(),
            path: vault_dir.to_string_lossy().to_string(),
            password: Some("webkeypass1".to_string()),
            create_new: true,
            keyfile: Some(kf1_str.clone()),
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

        // 2. Unlock with password only (no keyfile) must fail
        let unlock_fail_req = UnlockVaultRequest {
            path: vault_dir.to_string_lossy().to_string(),
            password: "webkeypass1".to_string(),
            keyfile: None,
            timeout_minutes: None,
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/unlock")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&unlock_fail_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // 3. Unlock with password and keyfile succeeds
        let unlock_ok_req = UnlockVaultRequest {
            path: vault_dir.to_string_lossy().to_string(),
            password: "webkeypass1".to_string(),
            keyfile: Some(kf1_str.clone()),
            timeout_minutes: None,
        };
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/vaults/unlock")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&unlock_ok_req).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 4. Change password and keyfile via API
        let chg_req = ChangePasswordRequest {
            vault_path: vault_dir.to_string_lossy().to_string(),
            old_password: "webkeypass1".to_string(),
            new_password: "webkeypass2".to_string(),
            old_keyfile: Some(kf1_str.clone()),
            new_keyfile: Some(kf2_str.clone()),
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

        // 5. Unlock with new credentials succeeds
        let unlock_new_req = UnlockVaultRequest {
            path: vault_dir.to_string_lossy().to_string(),
            password: "webkeypass2".to_string(),
            keyfile: Some(kf2_str.clone()),
            timeout_minutes: None,
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

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        save_saved_vaults(&original_saved);
    }

    #[tokio::test]
    async fn test_web_api_find_and_metadata() {
        let _guard = TEST_LOCK.lock().await;
        let original_saved = load_saved_vaults();
        let temp_dir = std::env::temp_dir().join("cryptomator_web_find_test");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let vault_dir = temp_dir.join("find_vault");
        crate::vault::create_vault(&vault_dir, "findpass123", None).unwrap();
        let key = crate::vault::unlock_vault(&vault_dir, "findpass123", None).unwrap();
        crate::vault::write_file(&vault_dir, &key, "docs/budget.pdf", b"PDF").unwrap();
        crate::vault::write_file(&vault_dir, &key, "images/photo.jpg", b"JPG").unwrap();

        let state = Arc::new(AppState::default());
        let router = build_router(state.clone());

        let unlock_req = UnlockVaultRequest {
            path: vault_dir.to_string_lossy().to_string(),
            password: "findpass123".to_string(),
            keyfile: None,
            timeout_minutes: None,
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

        // 1. GET /api/vaults/files -> verify items and modification date
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/vaults/files?path={}",
                        vault_dir.to_string_lossy()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let files_resp: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let items = files_resp.get("items").unwrap().as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].get("modified").is_some());

        // 2. GET /api/vaults/find?path=...&q=budget
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/vaults/find?path={}&q=budget",
                        vault_dir.to_string_lossy()
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let find_resp: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        let find_items = find_resp.get("items").unwrap().as_array().unwrap();
        assert_eq!(find_items.len(), 1);
        assert_eq!(find_items[0]["path"], "docs/budget.pdf");

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        save_saved_vaults(&original_saved);
    }

    #[tokio::test]
    async fn test_auto_lock_expiration() {
        let state = AppState::default();
        let key = [42u8; 32];
        let path = "/tmp/test_auto_lock_vault".to_string();

        // 1. Insert with 10s timeout, but last_activity was 15s ago
        state.unlocked_vaults.write().await.insert(
            path.clone(),
            UnlockedVaultInfo {
                master_key: key,
                last_activity: std::time::Instant::now() - std::time::Duration::from_secs(15),
                timeout_seconds: 10,
            },
        );

        // get_master_key must return None and remove the expired vault
        assert_eq!(state.get_master_key(&path).await, None);
        assert!(!state.unlocked_vaults.read().await.contains_key(&path));

        // 2. Insert with 0 (disabled auto-lock), last_activity 100s ago
        state.unlocked_vaults.write().await.insert(
            path.clone(),
            UnlockedVaultInfo {
                master_key: key,
                last_activity: std::time::Instant::now() - std::time::Duration::from_secs(100),
                timeout_seconds: 0,
            },
        );

        // get_master_key must return Some(key)
        assert_eq!(state.get_master_key(&path).await, Some(key));
    }

    #[tokio::test]
    async fn test_web_api_grep() {
        let _guard = TEST_LOCK.lock().await;
        let temp_dir = std::env::temp_dir().join("cryptomator_web_test_grep");
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
        let original_saved = load_saved_vaults();

        let state = Arc::new(AppState::default());
        let router = build_router(state.clone());

        // 1. Create and unlock vault
        let create_req = AddVaultRequest {
            name: "GrepTestVault".to_string(),
            path: temp_dir.to_string_lossy().to_string(),
            password: Some("greppass".to_string()),
            create_new: true,
            keyfile: None,
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

        let unlock_req = UnlockVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
            password: "greppass".to_string(),
            keyfile: None,
            timeout_minutes: None,
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

        // 2. Write file inside the vault
        let key = state
            .get_master_key(&temp_dir.to_string_lossy())
            .await
            .unwrap();
        crate::vault::write_file(
            &temp_dir,
            &key,
            "security.txt",
            b"Header line\nRust cryptography is fast\nFooter line",
        )
        .unwrap();

        // 3. Call grep endpoint with matching query
        let uri = format!(
            "/api/vaults/grep?path={}&q=cryptography",
            temp_dir.to_string_lossy()
        );
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let grep_res: Vec<crate::vault::FileGrepResult> = serde_json::from_slice(&body).unwrap();
        assert_eq!(grep_res.len(), 1);
        assert_eq!(grep_res[0].path, "security.txt");
        assert_eq!(grep_res[0].matches.len(), 1);
        assert_eq!(grep_res[0].matches[0].line_number, 2);
        assert_eq!(grep_res[0].matches[0].line, "Rust cryptography is fast");

        // 4. Call grep endpoint with non-matching query
        let uri = format!(
            "/api/vaults/grep?path={}&q=nonexistentword",
            temp_dir.to_string_lossy()
        );
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let grep_res: Vec<crate::vault::FileGrepResult> = serde_json::from_slice(&body).unwrap();
        assert!(grep_res.is_empty());

        // Cleanup
        save_saved_vaults(&original_saved);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_web_api_diff() {
        let _guard = TEST_LOCK.lock().await;
        let temp_dir = std::env::temp_dir().join("cryptomator_web_test_diff");
        if temp_dir.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
        let original_saved = load_saved_vaults();

        let state = Arc::new(AppState::default());
        let router = build_router(state.clone());

        // 1. Create and unlock vault
        let create_req = AddVaultRequest {
            name: "DiffTestVault".to_string(),
            path: temp_dir.to_string_lossy().to_string(),
            password: Some("diffpass".to_string()),
            create_new: true,
            keyfile: None,
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

        let unlock_req = UnlockVaultRequest {
            path: temp_dir.to_string_lossy().to_string(),
            password: "diffpass".to_string(),
            keyfile: None,
            timeout_minutes: None,
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

        // 2. Write two files
        let key = state
            .get_master_key(&temp_dir.to_string_lossy())
            .await
            .unwrap();
        crate::vault::write_file(
            &temp_dir,
            &key,
            "version1.txt",
            b"Line 1: A\nLine 2: B\nLine 3: C",
        )
        .unwrap();
        crate::vault::write_file(
            &temp_dir,
            &key,
            "version2.txt",
            b"Line 1: A\nLine 2: B modified\nLine 3: C",
        )
        .unwrap();

        // 3. Call diff endpoint
        let uri = format!(
            "/api/vaults/diff?path={}&file_a=version1.txt&file_b=version2.txt",
            temp_dir.to_string_lossy()
        );
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let diff_res: crate::vault::DiffResult = serde_json::from_slice(&body).unwrap();
        assert!(!diff_res.are_identical);
        assert_eq!(diff_res.additions, 1);
        assert_eq!(diff_res.deletions, 1);
        assert!(diff_res.unified_diff.contains("-Line 2: B"));
        assert!(diff_res.unified_diff.contains("+Line 2: B modified"));

        // Cleanup
        save_saved_vaults(&original_saved);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
