#![cfg(feature = "web")]

use bytes::{Buf, Bytes};
use dav_server::davpath::DavPath;
use dav_server::fs::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream,
    OpenOptions, ReadDirMeta,
};
use futures_util::future::FutureExt;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::vault;

fn clean_rel_path(dav_path: &DavPath) -> String {
    let p = dav_path.as_url_string();
    p.trim_start_matches('/').trim_end_matches('/').to_string()
}

#[derive(Clone, Debug)]
pub struct CryptomatorDavMetaData {
    pub is_dir: bool,
    pub len: u64,
    pub modified: SystemTime,
}

impl DavMetaData for CryptomatorDavMetaData {
    fn len(&self) -> u64 {
        self.len
    }

    fn modified(&self) -> FsResult<SystemTime> {
        Ok(self.modified)
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }
}

#[derive(Clone, Debug)]
pub struct CryptomatorDavDirEntry {
    pub name: String,
    pub is_dir: bool,
    pub len: u64,
    pub modified: SystemTime,
}

impl DavDirEntry for CryptomatorDavDirEntry {
    fn name(&self) -> Vec<u8> {
        self.name.as_bytes().to_vec()
    }

    fn metadata(&self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta = CryptomatorDavMetaData {
            is_dir: self.is_dir,
            len: self.len,
            modified: self.modified,
        };
        async move { Ok(Box::new(meta) as Box<dyn DavMetaData>) }.boxed()
    }
}

#[derive(Debug)]
pub struct CryptomatorDavFile {
    vault_dir: PathBuf,
    master_key: [u8; 32],
    rel_path: String,
    buffer: Vec<u8>,
    pos: usize,
    modified: bool,
    last_modified: SystemTime,
}

impl DavFile for CryptomatorDavFile {
    fn metadata(&mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta = CryptomatorDavMetaData {
            is_dir: false,
            len: self.buffer.len() as u64,
            modified: self.last_modified,
        };
        async move { Ok(Box::new(meta) as Box<dyn DavMetaData>) }.boxed()
    }

    fn write_buf(&mut self, mut buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()> {
        let count = buf.remaining();
        let bytes = buf.copy_to_bytes(count);
        self.write_bytes(bytes)
    }

    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<'_, ()> {
        let len = buf.len();
        let end = match self.pos.checked_add(len) {
            Some(e) => e,
            None => return async { Err(FsError::GeneralFailure) }.boxed(),
        };

        if end > self.buffer.len() {
            self.buffer.resize(end, 0);
        }
        self.buffer[self.pos..end].copy_from_slice(&buf);
        self.pos = end;
        self.modified = true;
        self.last_modified = SystemTime::now();

        async { Ok(()) }.boxed()
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes> {
        if self.pos >= self.buffer.len() {
            return async { Ok(Bytes::new()) }.boxed();
        }

        let available = self.buffer.len() - self.pos;
        let to_read = count.min(available);
        let slice = &self.buffer[self.pos..self.pos + to_read];
        let bytes = Bytes::copy_from_slice(slice);
        self.pos += to_read;

        async move { Ok(bytes) }.boxed()
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        let new_pos: Option<usize> = match pos {
            SeekFrom::Start(offset) => usize::try_from(offset).ok(),
            SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.pos.checked_add(offset as usize)
                } else {
                    let sub = match offset.checked_neg() {
                        Some(neg) => neg as usize,
                        None => return async { Err(FsError::GeneralFailure) }.boxed(),
                    };
                    self.pos.checked_sub(sub)
                }
            }
            SeekFrom::End(offset) => {
                let end = self.buffer.len();
                if offset >= 0 {
                    end.checked_add(offset as usize)
                } else {
                    let sub = match offset.checked_neg() {
                        Some(neg) => neg as usize,
                        None => return async { Err(FsError::GeneralFailure) }.boxed(),
                    };
                    end.checked_sub(sub)
                }
            }
        };

        match new_pos {
            Some(p) => {
                self.pos = p;
                async move { Ok(p as u64) }.boxed()
            }
            None => async { Err(FsError::GeneralFailure) }.boxed(),
        }
    }

    fn flush(&mut self) -> FsFuture<'_, ()> {
        if self.modified {
            let res = vault::write_file(
                &self.vault_dir,
                &self.master_key,
                &self.rel_path,
                &self.buffer,
            )
            .map_err(|_| FsError::GeneralFailure);
            if res.is_ok() {
                self.modified = false;
            }
            async move { res }.boxed()
        } else {
            async { Ok(()) }.boxed()
        }
    }
}

impl Drop for CryptomatorDavFile {
    fn drop(&mut self) {
        if self.modified {
            let _ = vault::write_file(
                &self.vault_dir,
                &self.master_key,
                &self.rel_path,
                &self.buffer,
            );
            self.modified = false;
        }
    }
}

#[derive(Clone, Debug)]
pub struct CryptomatorDavFs {
    vault_dir: PathBuf,
    master_key: [u8; 32],
}

impl CryptomatorDavFs {
    pub fn new(vault_dir: PathBuf, master_key: [u8; 32]) -> Self {
        Self {
            vault_dir,
            master_key,
        }
    }
}

impl DavFileSystem for CryptomatorDavFs {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        async move {
            let rel = clean_rel_path(path);
            if rel.is_empty() {
                return Err(FsError::Forbidden);
            }

            let exists = vault::read_file(&self.vault_dir, &self.master_key, &rel).is_ok();

            if options.create_new && exists {
                return Err(FsError::Exists);
            }

            let mut buffer = Vec::new();
            let mut last_modified = SystemTime::now();

            if exists && !options.truncate {
                match vault::read_file(&self.vault_dir, &self.master_key, &rel) {
                    Ok(data) => {
                        buffer = data;
                        if let Ok(files) =
                            vault::list_files_with_details(&self.vault_dir, &self.master_key)
                            && let Some(m) = files
                                .into_iter()
                                .find(|item| item.path == rel)
                                .and_then(|f| f.modified)
                        {
                            last_modified = UNIX_EPOCH + Duration::from_secs(m);
                        }
                    }
                    Err(_) => return Err(FsError::GeneralFailure),
                }
            } else if !exists && !options.create && !options.write {
                return Err(FsError::NotFound);
            }

            let file = CryptomatorDavFile {
                vault_dir: self.vault_dir.clone(),
                master_key: self.master_key,
                rel_path: rel,
                buffer,
                pos: 0,
                modified: options.truncate || (!exists && (options.create || options.write)),
                last_modified,
            };

            Ok(Box::new(file) as Box<dyn DavFile>)
        }
        .boxed()
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        async move {
            let rel = clean_rel_path(path);
            let files = vault::list_files_with_details(&self.vault_dir, &self.master_key)
                .map_err(|_| FsError::GeneralFailure)?;

            let prefix = if rel.is_empty() {
                String::new()
            } else {
                format!("{rel}/")
            };

            let mut subdirs: HashMap<String, SystemTime> = HashMap::new();
            let mut direct_files: Vec<CryptomatorDavDirEntry> = Vec::new();

            for item in files {
                if item.path.starts_with(".trash/")
                    || item.path.ends_with("/.keep")
                    || item.path == ".keep"
                {
                    continue;
                }

                if let Some(suffix) = item.path.strip_prefix(&prefix) {
                    let mut parts = suffix.split('/');
                    if let Some(first_part) = parts.next() {
                        let mtime = item
                            .modified
                            .map(|m| UNIX_EPOCH + Duration::from_secs(m))
                            .unwrap_or_else(SystemTime::now);

                        if parts.next().is_some() {
                            subdirs.entry(first_part.to_string()).or_insert(mtime);
                        } else {
                            direct_files.push(CryptomatorDavDirEntry {
                                name: first_part.to_string(),
                                is_dir: false,
                                len: item.size,
                                modified: mtime,
                            });
                        }
                    }
                }
            }

            let mut entries: Vec<Box<dyn DavDirEntry>> = Vec::new();

            for (dir_name, mtime) in subdirs {
                entries.push(Box::new(CryptomatorDavDirEntry {
                    name: dir_name,
                    is_dir: true,
                    len: 0,
                    modified: mtime,
                }));
            }

            for file_entry in direct_files {
                entries.push(Box::new(file_entry));
            }

            let stream = futures_util::stream::iter(entries.into_iter().map(Ok));
            Ok(Box::pin(stream) as FsStream<Box<dyn DavDirEntry>>)
        }
        .boxed()
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        async move {
            let rel = clean_rel_path(path);
            if rel.is_empty() {
                let root_meta = CryptomatorDavMetaData {
                    is_dir: true,
                    len: 0,
                    modified: SystemTime::now(),
                };
                return Ok(Box::new(root_meta) as Box<dyn DavMetaData>);
            }

            let files = vault::list_files_with_details(&self.vault_dir, &self.master_key)
                .map_err(|_| FsError::GeneralFailure)?;

            for item in &files {
                if item.path == rel {
                    let mtime = item
                        .modified
                        .map(|m| UNIX_EPOCH + Duration::from_secs(m))
                        .unwrap_or_else(SystemTime::now);
                    let meta = CryptomatorDavMetaData {
                        is_dir: false,
                        len: item.size,
                        modified: mtime,
                    };
                    return Ok(Box::new(meta) as Box<dyn DavMetaData>);
                }
            }

            let dir_prefix = format!("{rel}/");
            let mut is_directory = false;
            let mut dir_modified = SystemTime::now();

            for item in &files {
                if item.path.starts_with(&dir_prefix) {
                    is_directory = true;
                    if let Some(m) = item.modified {
                        dir_modified = UNIX_EPOCH + Duration::from_secs(m);
                    }
                    break;
                }
            }

            if is_directory {
                let meta = CryptomatorDavMetaData {
                    is_dir: true,
                    len: 0,
                    modified: dir_modified,
                };
                return Ok(Box::new(meta) as Box<dyn DavMetaData>);
            }

            Err(FsError::NotFound)
        }
        .boxed()
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let rel = clean_rel_path(path);
            if rel.is_empty() {
                return Ok(());
            }
            vault::create_dir(&self.vault_dir, &self.master_key, &rel)
                .map_err(|_| FsError::GeneralFailure)
        }
        .boxed()
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let rel = clean_rel_path(path);
            if rel.is_empty() {
                return Err(FsError::Forbidden);
            }
            vault::delete_dir(&self.vault_dir, &self.master_key, &rel)
                .map(|_| ())
                .map_err(|_| FsError::GeneralFailure)
        }
        .boxed()
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let rel = clean_rel_path(path);
            vault::delete_file(&self.vault_dir, &self.master_key, &rel)
                .map_err(|_| FsError::NotFound)
        }
        .boxed()
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let from_rel = clean_rel_path(from);
            let to_rel = clean_rel_path(to);
            let meta = self.metadata(from).await?;
            if meta.is_dir() {
                vault::rename_dir(&self.vault_dir, &self.master_key, &from_rel, &to_rel)
                    .map_err(|_| FsError::GeneralFailure)
            } else {
                vault::rename_file(&self.vault_dir, &self.master_key, &from_rel, &to_rel)
                    .map_err(|_| FsError::GeneralFailure)
            }
        }
        .boxed()
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let from_rel = clean_rel_path(from);
            let to_rel = clean_rel_path(to);
            let meta = self.metadata(from).await?;
            if meta.is_dir() {
                vault::copy_dir(&self.vault_dir, &self.master_key, &from_rel, &to_rel)
                    .map(|_| ())
                    .map_err(|_| FsError::GeneralFailure)
            } else {
                vault::copy_file(&self.vault_dir, &self.master_key, &from_rel, &to_rel)
                    .map_err(|_| FsError::GeneralFailure)
            }
        }
        .boxed()
    }

    fn get_quota<'a>(&'a self) -> FsFuture<'a, (u64, Option<u64>)> {
        async move {
            let used = match vault::get_vault_stats(&self.vault_dir, &self.master_key) {
                Ok(stats) => stats.total_size_bytes,
                Err(_) => 0,
            };
            // Return 1 TB quota limit for network drive display
            Ok((used, Some(1024 * 1024 * 1024 * 1024)))
        }
        .boxed()
    }
}

pub fn create_webdav_handler(
    vault_dir: PathBuf,
    master_key: [u8; 32],
    prefix: Option<&str>,
) -> dav_server::DavHandler {
    let fs = CryptomatorDavFs::new(vault_dir, master_key);
    let mut builder = dav_server::DavHandler::builder()
        .filesystem(Box::new(fs))
        .locksystem(dav_server::fakels::FakeLs::new());

    if let Some(p) = prefix {
        builder = builder.strip_prefix(p);
    }

    builder.build_handler()
}

pub async fn run_webdav_server(
    vault_dir: PathBuf,
    master_key: [u8; 32],
    host: &str,
    port: u16,
) -> Result<(), String> {
    let dav_handler = Arc::new(create_webdav_handler(vault_dir, master_key, None));

    let app = axum::Router::new().fallback(move |req: axum::extract::Request| {
        let handler = dav_handler.clone();
        async move {
            let (parts, body) = handler.handle(req).await.into_parts();
            axum::response::Response::from_parts(parts, axum::body::Body::new(body))
        }
    });

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind WebDAV listener on {addr}: {e}"))?;

    println!("WebDAV server running on http://{addr}/");
    println!("Native operating system mount guides:");
    println!("  • Windows:      Map Network Drive -> http://{addr}/");
    println!("  • macOS Finder: Connect to Server -> http://{addr}/");
    println!("  • Linux:        Connect to Server -> dav://{addr}/ (in GNOME Files / Dolphin)");

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("WebDAV server terminated: {e}"))?;

    Ok(())
}
