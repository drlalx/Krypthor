use crate::vault;
use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyWrite,
    Request,
};
use libc::{EEXIST, EISDIR, ENOENT, ENOTDIR, ENOTEMPTY};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const TTL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub enum FSNode {
    Directory {
        inode: u64,
        parent: u64,
        name: String,
        children: HashMap<String, u64>, // name -> inode
    },
    File {
        inode: u64,
        parent: u64,
        name: String,
        relative_path: String,    // Full logical path, e.g. "objects/ab/1234"
        content: Option<Vec<u8>>, // Cached decrypted content
        size: usize,
        dirty: bool,
    },
}

pub struct CryptomatorFS {
    vault_dir: PathBuf,
    master_key: [u8; 32],
    inodes: HashMap<u64, FSNode>,
    next_inode: u64,
}

impl CryptomatorFS {
    pub fn new(vault_dir: PathBuf, master_key: [u8; 32]) -> Self {
        let mut fs = Self {
            vault_dir,
            master_key,
            inodes: HashMap::new(),
            next_inode: 2,
        };
        let _ = fs.refresh_tree();
        fs
    }

    fn refresh_tree(&mut self) -> Result<(), String> {
        self.inodes.clear();
        self.inodes.insert(
            1,
            FSNode::Directory {
                inode: 1,
                parent: 1,
                name: "".to_string(),
                children: HashMap::new(),
            },
        );
        self.next_inode = 2;

        let files = vault::list_files(&self.vault_dir, &self.master_key)?;
        for rel_path in files {
            let parts: Vec<&str> = rel_path.split('/').collect();
            let mut current_inode = 1;

            for (i, part) in parts.iter().enumerate() {
                let is_last = i == parts.len() - 1;

                let children = match self.inodes.get_mut(&current_inode) {
                    Some(FSNode::Directory { children, .. }) => children,
                    _ => return Err("Invalid tree structure".to_string()),
                };

                if let Some(&child_ino) = children.get(*part) {
                    current_inode = child_ino;
                } else {
                    let new_ino = self.next_inode;
                    self.next_inode += 1;

                    if is_last {
                        let phys_name = vault::get_physical_filename(&self.master_key, &rel_path);
                        let phys_path = self.vault_dir.join("d").join(phys_name);
                        let size = get_decrypted_file_size(&phys_path).unwrap_or(0);

                        self.inodes.insert(
                            new_ino,
                            FSNode::File {
                                inode: new_ino,
                                parent: current_inode,
                                name: part.to_string(),
                                relative_path: rel_path.clone(),
                                content: None,
                                size,
                                dirty: false,
                            },
                        );
                    } else {
                        self.inodes.insert(
                            new_ino,
                            FSNode::Directory {
                                inode: new_ino,
                                parent: current_inode,
                                name: part.to_string(),
                                children: HashMap::new(),
                            },
                        );
                    }

                    // Re-borrow parent's children to insert
                    let children = match self.inodes.get_mut(&current_inode) {
                        Some(FSNode::Directory { children, .. }) => children,
                        _ => return Err("Invalid directory node".to_string()),
                    };
                    children.insert(part.to_string(), new_ino);
                    current_inode = new_ino;
                }
            }
        }
        Ok(())
    }

    fn load_file_content(&mut self, inode: u64) -> Result<Vec<u8>, String> {
        let (rel_path, content_opt) = match self.inodes.get(&inode) {
            Some(FSNode::File {
                relative_path,
                content,
                ..
            }) => (relative_path.clone(), content.clone()),
            _ => return Err("Not a file".to_string()),
        };

        if let Some(content) = content_opt {
            return Ok(content);
        }

        let content = vault::read_file(&self.vault_dir, &self.master_key, &rel_path)?;

        if let Some(FSNode::File {
            content: cached,
            size,
            ..
        }) = self.inodes.get_mut(&inode)
        {
            *cached = Some(content.clone());
            *size = content.len();
        }

        Ok(content)
    }

    fn write_file_to_disk(&mut self, inode: u64) -> Result<(), String> {
        let (rel_path, content) = match self.inodes.get(&inode) {
            Some(FSNode::File {
                relative_path,
                content: Some(content),
                dirty: true,
                ..
            }) => (relative_path.clone(), content.clone()),
            _ => return Ok(()), // Not dirty or not loaded
        };

        vault::write_file(&self.vault_dir, &self.master_key, &rel_path, &content)?;

        if let Some(FSNode::File { dirty, .. }) = self.inodes.get_mut(&inode) {
            *dirty = false;
        }

        Ok(())
    }

    fn rename_recursive(
        &mut self,
        inode: u64,
        old_prefix: &str,
        new_prefix: &str,
    ) -> Result<(), String> {
        // Collect children to avoid borrow checker issues
        let node_copy = self.inodes.get(&inode).cloned();

        match node_copy {
            Some(FSNode::File { relative_path, .. }) => {
                // Determine new relative path
                if let Some(suffix) = relative_path.strip_prefix(old_prefix) {
                    let new_relative_path = format!("{}{}", new_prefix, suffix);

                    // We must read content first before deleting
                    let _ = self.load_file_content(inode)?;

                    // Delete old physical file
                    let _ = vault::delete_file(&self.vault_dir, &self.master_key, &relative_path);

                    // Update in-memory node
                    if let Some(FSNode::File {
                        relative_path: path_ref,
                        dirty,
                        ..
                    }) = self.inodes.get_mut(&inode)
                    {
                        *path_ref = new_relative_path.clone();
                        *dirty = true;
                    }

                    // Write immediately
                    self.write_file_to_disk(inode)?;
                }
            }
            Some(FSNode::Directory { children, .. }) => {
                for &child_ino in children.values() {
                    self.rename_recursive(child_ino, old_prefix, new_prefix)?;
                }
            }
            None => {}
        }
        Ok(())
    }

    fn get_node_path(&self, inode: u64) -> String {
        let mut parts = Vec::new();
        let mut curr = inode;
        while curr != 1 {
            match self.inodes.get(&curr) {
                Some(FSNode::File { name, parent, .. }) => {
                    parts.push(name.clone());
                    curr = *parent;
                }
                Some(FSNode::Directory { name, parent, .. }) => {
                    parts.push(name.clone());
                    curr = *parent;
                }
                None => break,
            }
        }
        parts.reverse();
        parts.join("/")
    }
}

fn get_decrypted_file_size(phys_path: &Path) -> io::Result<usize> {
    let mut file = File::open(phys_path)?;
    let mut header = [0u8; 16];
    if file.read_exact(&mut header).is_err() {
        return Ok(0);
    }
    let path_len = u32::from_be_bytes([header[12], header[13], header[14], header[15]]) as usize;
    let file_len = file.metadata()?.len() as usize;
    if file_len < 32 + path_len {
        return Ok(0);
    }
    Ok(file_len - 32 - path_len)
}

fn make_attr(ino: u64, size: u64, is_dir: bool) -> FileAttr {
    let now = SystemTime::now();
    FileAttr {
        ino,
        size,
        blocks: if is_dir { 0 } else { size.div_ceil(512) },
        atime: now,
        mtime: now,
        ctime: now,
        crtime: now,
        kind: if is_dir {
            FileType::Directory
        } else {
            FileType::RegularFile
        },
        perm: if is_dir { 0o755 } else { 0o644 },
        nlink: if is_dir { 2 } else { 1 },
        uid: unsafe { libc::getuid() },
        gid: unsafe { libc::getgid() },
        rdev: 0,
        blksize: 512,
        flags: 0,
    }
}

impl Filesystem for CryptomatorFS {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &std::ffi::OsStr, reply: ReplyEntry) {
        let name_str = name.to_string_lossy().to_string();

        if let Some(FSNode::Directory { children, .. }) = self.inodes.get(&parent)
            && let Some(&child_ino) = children.get(&name_str)
            && let Some(node) = self.inodes.get(&child_ino)
        {
            match node {
                FSNode::Directory { inode, .. } => {
                    reply.entry(&TTL, &make_attr(*inode, 4096, true), 0);
                    return;
                }
                FSNode::File { inode, size, .. } => {
                    reply.entry(&TTL, &make_attr(*inode, *size as u64, false), 0);
                    return;
                }
            }
        }
        reply.error(ENOENT);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        if let Some(node) = self.inodes.get(&ino) {
            match node {
                FSNode::Directory { inode, .. } => {
                    reply.attr(&TTL, &make_attr(*inode, 4096, true));
                }
                FSNode::File { inode, size, .. } => {
                    reply.attr(&TTL, &make_attr(*inode, *size as u64, false));
                }
            }
        } else {
            reply.error(ENOENT);
        }
    }

    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<fuser::TimeOrNow>,
        _mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        if let Some(new_size) = size {
            let new_size_usize = new_size as usize;

            // Load and truncate
            match self.load_file_content(ino) {
                Ok(mut content) => {
                    content.resize(new_size_usize, 0);

                    if let Some(FSNode::File {
                        content: cached,
                        size: node_size,
                        dirty,
                        ..
                    }) = self.inodes.get_mut(&ino)
                    {
                        *cached = Some(content);
                        *node_size = new_size_usize;
                        *dirty = true;
                    }

                    let _ = self.write_file_to_disk(ino);
                }
                Err(_) => {
                    reply.error(ENOENT);
                    return;
                }
            }
        }

        if let Some(node) = self.inodes.get(&ino) {
            match node {
                FSNode::Directory { inode, .. } => {
                    reply.attr(&TTL, &make_attr(*inode, 4096, true));
                }
                FSNode::File { inode, size, .. } => {
                    reply.attr(&TTL, &make_attr(*inode, *size as u64, false));
                }
            }
        } else {
            reply.error(ENOENT);
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        match self.load_file_content(ino) {
            Ok(content) => {
                let offset = offset as usize;
                let size = size as usize;

                if offset >= content.len() {
                    reply.data(&[]);
                } else {
                    let end = std::cmp::min(offset + size, content.len());
                    reply.data(&content[offset..end]);
                }
            }
            Err(_) => reply.error(ENOENT),
        }
    }

    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyWrite,
    ) {
        match self.load_file_content(ino) {
            Ok(mut content) => {
                let offset = offset as usize;

                if offset + data.len() > content.len() {
                    content.resize(offset + data.len(), 0);
                }

                content[offset..offset + data.len()].copy_from_slice(data);
                let bytes_written = data.len();

                if let Some(FSNode::File {
                    content: cached,
                    size: node_size,
                    dirty,
                    ..
                }) = self.inodes.get_mut(&ino)
                {
                    *cached = Some(content);
                    *node_size = offset + bytes_written;
                    *dirty = true;
                }

                // Write immediately for reliability over network
                match self.write_file_to_disk(ino) {
                    Ok(_) => reply.written(bytes_written as u32),
                    Err(_) => reply.error(libc::EIO),
                }
            }
            Err(_) => reply.error(ENOENT),
        }
    }

    fn flush(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        match self.write_file_to_disk(ino) {
            Ok(_) => reply.ok(),
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn release(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        match self.write_file_to_disk(ino) {
            Ok(_) => reply.ok(),
            Err(_) => reply.error(libc::EIO),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if offset == 0 {
            let mut entries = vec![(ino, FileType::Directory, ".".to_string())];

            if let Some(node) = self.inodes.get(&ino) {
                let parent_ino = match node {
                    FSNode::Directory { parent, .. } => *parent,
                    FSNode::File { parent, .. } => *parent,
                };
                entries.push((parent_ino, FileType::Directory, "..".to_string()));

                if let FSNode::Directory { children, .. } = node {
                    for (name, &child_ino) in children {
                        if let Some(child_node) = self.inodes.get(&child_ino) {
                            let kind = match child_node {
                                FSNode::Directory { .. } => FileType::Directory,
                                FSNode::File { .. } => FileType::RegularFile,
                            };
                            entries.push((child_ino, kind, name.clone()));
                        }
                    }
                }
            }

            for (i, (entry_ino, entry_kind, entry_name)) in entries.iter().enumerate() {
                let buffer_full = reply.add(*entry_ino, (i + 1) as i64, *entry_kind, entry_name);
                if buffer_full {
                    break;
                }
            }
            reply.ok();
        } else {
            reply.ok();
        }
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let name_str = name.to_string_lossy().to_string();

        // Check parent directory
        let has_child = match self.inodes.get(&parent) {
            Some(FSNode::Directory { children, .. }) => children.contains_key(&name_str),
            _ => {
                reply.error(ENOTDIR);
                return;
            }
        };

        if has_child {
            reply.error(EEXIST);
            return;
        }

        let new_ino = self.next_inode;
        self.next_inode += 1;

        self.inodes.insert(
            new_ino,
            FSNode::Directory {
                inode: new_ino,
                parent,
                name: name_str.clone(),
                children: HashMap::new(),
            },
        );

        if let Some(FSNode::Directory { children, .. }) = self.inodes.get_mut(&parent) {
            children.insert(name_str, new_ino);
        }

        reply.entry(&TTL, &make_attr(new_ino, 4096, true), 0);
    }

    fn rmdir(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let name_str = name.to_string_lossy().to_string();

        let child_ino = match self.inodes.get(&parent) {
            Some(FSNode::Directory { children, .. }) => children.get(&name_str).cloned(),
            _ => {
                reply.error(ENOTDIR);
                return;
            }
        };

        let child_ino = match child_ino {
            Some(ino) => ino,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Check if empty
        let is_empty = match self.inodes.get(&child_ino) {
            Some(FSNode::Directory { children, .. }) => children.is_empty(),
            _ => {
                reply.error(ENOTDIR);
                return;
            }
        };

        if !is_empty {
            reply.error(ENOTEMPTY);
            return;
        }

        // Remove
        self.inodes.remove(&child_ino);
        if let Some(FSNode::Directory { children, .. }) = self.inodes.get_mut(&parent) {
            children.remove(&name_str);
        }

        reply.ok();
    }

    fn unlink(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &std::ffi::OsStr,
        reply: fuser::ReplyEmpty,
    ) {
        let name_str = name.to_string_lossy().to_string();

        let child_ino = match self.inodes.get(&parent) {
            Some(FSNode::Directory { children, .. }) => children.get(&name_str).cloned(),
            _ => {
                reply.error(ENOTDIR);
                return;
            }
        };

        let child_ino = match child_ino {
            Some(ino) => ino,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let rel_path = match self.inodes.get(&child_ino) {
            Some(FSNode::File { relative_path, .. }) => relative_path.clone(),
            _ => {
                reply.error(EISDIR);
                return;
            }
        };

        // Delete from vault
        let _ = vault::delete_file(&self.vault_dir, &self.master_key, &rel_path);

        // Remove in-memory
        self.inodes.remove(&child_ino);
        if let Some(FSNode::Directory { children, .. }) = self.inodes.get_mut(&parent) {
            children.remove(&name_str);
        }

        reply.ok();
    }

    fn create(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &std::ffi::OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        let name_str = name.to_string_lossy().to_string();

        let has_child = match self.inodes.get(&parent) {
            Some(FSNode::Directory { children, .. }) => children.contains_key(&name_str),
            _ => {
                reply.error(ENOTDIR);
                return;
            }
        };

        if has_child {
            reply.error(EEXIST);
            return;
        }

        let parent_path = self.get_node_path(parent);
        let rel_path = if parent_path.is_empty() {
            name_str.clone()
        } else {
            format!("{}/{}", parent_path, name_str)
        };

        let new_ino = self.next_inode;
        self.next_inode += 1;

        self.inodes.insert(
            new_ino,
            FSNode::File {
                inode: new_ino,
                parent,
                name: name_str.clone(),
                relative_path: rel_path.clone(),
                content: Some(Vec::new()),
                size: 0,
                dirty: true,
            },
        );

        if let Some(FSNode::Directory { children, .. }) = self.inodes.get_mut(&parent) {
            children.insert(name_str, new_ino);
        }

        // Write empty file immediately
        let _ = self.write_file_to_disk(new_ino);

        reply.created(&TTL, &make_attr(new_ino, 0, false), 0, 0, 0);
    }

    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &std::ffi::OsStr,
        newparent: u64,
        newname: &std::ffi::OsStr,
        _flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        let name_str = name.to_string_lossy().to_string();
        let newname_str = newname.to_string_lossy().to_string();

        let child_ino = match self.inodes.get(&parent) {
            Some(FSNode::Directory { children, .. }) => children.get(&name_str).cloned(),
            _ => {
                reply.error(ENOTDIR);
                return;
            }
        };

        let child_ino = match child_ino {
            Some(ino) => ino,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // If target exists, delete it first
        let target_exist_ino = match self.inodes.get(&newparent) {
            Some(FSNode::Directory { children, .. }) => children.get(&newname_str).cloned(),
            _ => {
                reply.error(ENOTDIR);
                return;
            }
        };

        if let Some(target_ino) = target_exist_ino {
            let is_dir = matches!(self.inodes.get(&target_ino), Some(FSNode::Directory { .. }));
            if is_dir {
                reply.error(EISDIR);
                return;
            }
            // Delete target file physically
            if let Some(FSNode::File { relative_path, .. }) = self.inodes.get(&target_ino) {
                let _ = vault::delete_file(&self.vault_dir, &self.master_key, relative_path);
            }
            self.inodes.remove(&target_ino);
        }

        // Compute prefixes for rename
        let old_prefix = self.get_node_path(child_ino);
        let newparent_path = self.get_node_path(newparent);
        let new_prefix = if newparent_path.is_empty() {
            newname_str.clone()
        } else {
            format!("{}/{}", newparent_path, newname_str)
        };

        // Rename recursively (updates paths and physical files for files/subfolders)
        if self
            .rename_recursive(child_ino, &old_prefix, &new_prefix)
            .is_err()
        {
            reply.error(libc::EIO);
            return;
        }

        // Update node parent and name
        if let Some(node) = self.inodes.get_mut(&child_ino) {
            match node {
                FSNode::File {
                    parent: p, name: n, ..
                } => {
                    *p = newparent;
                    *n = newname_str.clone();
                }
                FSNode::Directory {
                    parent: p, name: n, ..
                } => {
                    *p = newparent;
                    *n = newname_str.clone();
                }
            }
        }

        // Update children mappings
        if let Some(FSNode::Directory { children, .. }) = self.inodes.get_mut(&parent) {
            children.remove(&name_str);
        }

        if let Some(FSNode::Directory { children, .. }) = self.inodes.get_mut(&newparent) {
            children.insert(newname_str, child_ino);
        }

        reply.ok();
    }
}

/// Unmount an existing FUSE mountpoint cleanly using fusermount or umount
pub fn unmount_mountpoint(mountpoint: &Path, force: bool) -> Result<(), String> {
    let path_str = mountpoint.to_string_lossy().to_string();

    let mut fusermount_args = vec!["-u"];
    if force {
        fusermount_args.push("-z");
    }
    fusermount_args.push(&path_str);

    // 1. Try fusermount3
    if std::process::Command::new("fusermount3")
        .args(&fusermount_args)
        .status()
        .is_ok_and(|s| s.success())
    {
        return Ok(());
    }

    // 2. Try fusermount
    if std::process::Command::new("fusermount")
        .args(&fusermount_args)
        .status()
        .is_ok_and(|s| s.success())
    {
        return Ok(());
    }

    // 3. Fallback to umount
    let mut umount_args = Vec::new();
    if force {
        umount_args.push("-f");
    }
    umount_args.push(&path_str);

    if std::process::Command::new("umount")
        .args(&umount_args)
        .status()
        .is_ok_and(|s| s.success())
    {
        return Ok(());
    }

    Err(format!(
        "Failed to unmount '{path_str}' via fusermount3, fusermount, or umount"
    ))
}

/// Check if a path is currently registered as a mountpoint in the system
pub fn is_mountpoint(mountpoint: &Path) -> bool {
    let Ok(canonical) = mountpoint.canonicalize() else {
        return false;
    };
    let target = canonical.to_string_lossy();

    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == target {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn test_fuse_tree_construction_and_read() {
        let test_dir = std::env::temp_dir().join("cryptomator_fuse_tree_test");
        let _ = std::fs::remove_dir_all(&test_dir);

        vault::create_vault(&test_dir, "fusetest_pwd", None).unwrap();
        let master_key = vault::unlock_vault(&test_dir, "fusetest_pwd", None).unwrap();

        vault::write_file(&test_dir, &master_key, "readme.txt", b"Hello FUSE").unwrap();
        vault::write_file(&test_dir, &master_key, "docs/manual.md", b"# FUSE Manual").unwrap();
        vault::write_file(
            &test_dir,
            &master_key,
            "docs/sub/arch.txt",
            b"Pure Rust Architecture",
        )
        .unwrap();

        let mut fs = CryptomatorFS::new(test_dir.clone(), master_key);

        // 1. Verify root inode children
        let root = fs.inodes.get(&1).unwrap();
        if let FSNode::Directory { children, .. } = root {
            assert!(children.contains_key("readme.txt"));
            assert!(children.contains_key("docs"));
            assert_eq!(children.len(), 2);

            let docs_ino = children.get("docs").unwrap();
            let docs_node = fs.inodes.get(docs_ino).unwrap();
            if let FSNode::Directory {
                children: docs_children,
                ..
            } = docs_node
            {
                assert!(docs_children.contains_key("manual.md"));
                assert!(docs_children.contains_key("sub"));

                let sub_ino = docs_children.get("sub").unwrap();
                let sub_node = fs.inodes.get(sub_ino).unwrap();
                if let FSNode::Directory {
                    children: sub_children,
                    ..
                } = sub_node
                {
                    assert!(sub_children.contains_key("arch.txt"));
                    let arch_ino = sub_children.get("arch.txt").unwrap();

                    // Check path resolution
                    assert_eq!(fs.get_node_path(*arch_ino), "docs/sub/arch.txt");

                    // Read content
                    let content = fs.load_file_content(*arch_ino).unwrap();
                    assert_eq!(content, b"Pure Rust Architecture");
                } else {
                    panic!("sub should be directory");
                }
            } else {
                panic!("docs should be directory");
            }
        } else {
            panic!("inode 1 should be root directory");
        }

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_fuse_write_and_flush() {
        let test_dir = std::env::temp_dir().join("cryptomator_fuse_write_test");
        let _ = std::fs::remove_dir_all(&test_dir);

        vault::create_vault(&test_dir, "fusetest_pwd", None).unwrap();
        let master_key = vault::unlock_vault(&test_dir, "fusetest_pwd", None).unwrap();

        vault::write_file(&test_dir, &master_key, "notes.txt", b"Original notes").unwrap();

        let mut fs = CryptomatorFS::new(test_dir.clone(), master_key);
        let root = fs.inodes.get(&1).unwrap();
        let note_ino = match root {
            FSNode::Directory { children, .. } => *children.get("notes.txt").unwrap(),
            _ => panic!("Expected root dir"),
        };

        // Modify in-memory node and mark dirty
        if let Some(FSNode::File {
            content,
            size,
            dirty,
            ..
        }) = fs.inodes.get_mut(&note_ino)
        {
            *content = Some(b"Modified FUSE notes!".to_vec());
            *size = b"Modified FUSE notes!".len();
            *dirty = true;
        }

        // Flush to disk
        fs.write_file_to_disk(note_ino).unwrap();

        // Read directly from vault to ensure physical disk encryption
        let disk_content = vault::read_file(&test_dir, &master_key, "notes.txt").unwrap();
        assert_eq!(disk_content, b"Modified FUSE notes!");

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_fuse_rename_recursive() {
        let test_dir = std::env::temp_dir().join("cryptomator_fuse_rename_test");
        let _ = std::fs::remove_dir_all(&test_dir);

        vault::create_vault(&test_dir, "fusetest_pwd", None).unwrap();
        let master_key = vault::unlock_vault(&test_dir, "fusetest_pwd", None).unwrap();

        vault::write_file(
            &test_dir,
            &master_key,
            "projects/rust/main.rs",
            b"fn main() {}",
        )
        .unwrap();

        let mut fs = CryptomatorFS::new(test_dir.clone(), master_key);
        let root = fs.inodes.get(&1).unwrap();
        let projects_ino = match root {
            FSNode::Directory { children, .. } => *children.get("projects").unwrap(),
            _ => panic!("Expected root dir"),
        };

        // Rename projects -> workspace
        fs.rename_recursive(projects_ino, "projects", "workspace")
            .unwrap();

        // Check vault file listing
        let files = vault::list_files(&test_dir, &master_key).unwrap();
        assert_eq!(files, vec!["workspace/rust/main.rs"]);

        let content = vault::read_file(&test_dir, &master_key, "workspace/rust/main.rs").unwrap();
        assert_eq!(content, b"fn main() {}");

        let _ = std::fs::remove_dir_all(&test_dir);
    }
}
