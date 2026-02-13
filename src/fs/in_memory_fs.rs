//! In-Memory File System Implementation
//!
//! A pure in-memory virtual file system for the bash environment.

use std::collections::HashMap;
use std::collections::HashSet;
use std::time::SystemTime;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::types::*;

/// In-memory virtual file system.
pub struct InMemoryFs {
    data: RwLock<HashMap<String, FsEntry>>,
}

impl InMemoryFs {
    /// Create a new empty in-memory filesystem.
    pub fn new() -> Self {
        let mut data = HashMap::new();
        data.insert("/".to_string(), FsEntry::Directory {
            mode: 0o755,
            mtime: SystemTime::now(),
        });
        Self { data: RwLock::new(data) }
    }

    /// Create with initial files.
    pub fn with_files(files: &InitialFiles) -> Self {
        let fs = Self::new();
        let mut data = fs.data.blocking_write();
        for (path, init) in files {
            let normalized = normalize_path(path);
            ensure_parent_dirs(&mut data, &normalized);
            let content = match &init.content {
                FileContent::Text(s) => s.as_bytes().to_vec(),
                FileContent::Binary(b) => b.clone(),
            };
            data.insert(normalized, FsEntry::File {
                content,
                mode: init.mode.unwrap_or(0o644),
                mtime: init.mtime.unwrap_or_else(SystemTime::now),
            });
        }
        drop(data);
        fs
    }

    /// Synchronous mkdir for initialization.
    pub fn mkdir_sync(&self, path: &str) {
        let mut data = self.data.blocking_write();
        let normalized = normalize_path(path);
        let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();
        let mut current = String::new();
        for part in parts {
            current = format!("{}/{}", current, part);
            if !data.contains_key(&current) {
                data.insert(current.clone(), FsEntry::Directory {
                    mode: 0o755,
                    mtime: SystemTime::now(),
                });
            }
        }
    }

    /// Synchronous write_file for initialization.
    pub fn write_file_sync(&self, path: &str, content: &[u8]) {
        let mut data = self.data.blocking_write();
        let normalized = normalize_path(path);
        ensure_parent_dirs(&mut data, &normalized);
        data.insert(normalized, FsEntry::File {
            content: content.to_vec(),
            mode: 0o644,
            mtime: SystemTime::now(),
        });
    }
}

impl Default for InMemoryFs {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Path utilities (free functions operating on HashMap directly)
// ============================================================================

fn normalize_path(path: &str) -> String {
    if path.is_empty() || path == "/" {
        return "/".to_string();
    }
    let mut normalized = path.to_string();
    if normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    if !normalized.starts_with('/') {
        normalized = format!("/{}", normalized);
    }
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let mut resolved: Vec<&str> = Vec::new();
    for part in parts {
        if part == ".." {
            resolved.pop();
        } else {
            resolved.push(part);
        }
    }
    if resolved.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", resolved.join("/"))
    }
}

fn dirname(path: &str) -> String {
    let normalized = normalize_path(path);
    if normalized == "/" {
        return "/".to_string();
    }
    match normalized.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => normalized[..pos].to_string(),
        None => "/".to_string(),
    }
}

fn ensure_parent_dirs(data: &mut HashMap<String, FsEntry>, path: &str) {
    let dir = dirname(path);
    if dir == "/" {
        return;
    }
    if !data.contains_key(&dir) {
        ensure_parent_dirs(data, &dir);
        data.insert(dir, FsEntry::Directory {
            mode: 0o755,
            mtime: SystemTime::now(),
        });
    }
}

fn resolve_symlink_target(symlink_path: &str, target: &str) -> String {
    if target.starts_with('/') {
        normalize_path(target)
    } else {
        let dir = dirname(symlink_path);
        if dir == "/" {
            normalize_path(&format!("/{}", target))
        } else {
            normalize_path(&format!("{}/{}", dir, target))
        }
    }
}

/// Resolve all symlinks in a path (including intermediate components).
fn resolve_path_with_symlinks(
    data: &HashMap<String, FsEntry>,
    path: &str,
    operation: &str,
) -> Result<String, FsError> {
    let normalized = normalize_path(path);
    if normalized == "/" {
        return Ok("/".to_string());
    }
    let parts: Vec<&str> = normalized[1..].split('/').collect();
    let mut resolved = String::new();
    let mut seen = HashSet::new();

    for part in parts {
        resolved = format!("{}/{}", resolved, part);
        let mut entry = data.get(&resolved);
        let mut loop_count = 0;
        const MAX_LOOPS: usize = 40;

        while let Some(FsEntry::Symlink { target, .. }) = entry {
            if loop_count >= MAX_LOOPS || seen.contains(&resolved) {
                return Err(FsError::SymlinkLoop {
                    path: path.to_string(),
                    operation: operation.to_string(),
                });
            }
            seen.insert(resolved.clone());
            resolved = resolve_symlink_target(&resolved, target);
            entry = data.get(&resolved);
            loop_count += 1;
        }
    }
    Ok(resolved)
}

/// Resolve intermediate symlinks only (not the final component). Used by lstat.
fn resolve_intermediate_symlinks(
    data: &HashMap<String, FsEntry>,
    path: &str,
    operation: &str,
) -> Result<String, FsError> {
    let normalized = normalize_path(path);
    if normalized == "/" {
        return Ok("/".to_string());
    }
    let parts: Vec<&str> = normalized[1..].split('/').collect();
    if parts.len() <= 1 {
        return Ok(normalized);
    }

    let mut resolved = String::new();
    let mut seen = HashSet::new();

    // Process all but the last component
    for part in &parts[..parts.len() - 1] {
        resolved = format!("{}/{}", resolved, part);
        let mut entry = data.get(&resolved);
        let mut loop_count = 0;
        const MAX_LOOPS: usize = 40;

        while let Some(FsEntry::Symlink { target, .. }) = entry {
            if loop_count >= MAX_LOOPS || seen.contains(&resolved) {
                return Err(FsError::SymlinkLoop {
                    path: path.to_string(),
                    operation: operation.to_string(),
                });
            }
            seen.insert(resolved.clone());
            resolved = resolve_symlink_target(&resolved, target);
            entry = data.get(&resolved);
            loop_count += 1;
        }
    }

    // Append final component without resolving
    Ok(format!("{}/{}", resolved, parts[parts.len() - 1]))
}

// ============================================================================
// FileSystem trait implementation
// ============================================================================

#[async_trait]
impl FileSystem for InMemoryFs {
    async fn read_file(&self, path: &str) -> Result<String, FsError> {
        let buf = self.read_file_buffer(path).await?;
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    async fn read_file_buffer(&self, path: &str) -> Result<Vec<u8>, FsError> {
        let data = self.data.read().await;
        let resolved = resolve_path_with_symlinks(&data, path, "open")?;
        match data.get(&resolved) {
            Some(FsEntry::File { content, .. }) => Ok(content.clone()),
            Some(FsEntry::Directory { .. }) => Err(FsError::IsDirectory {
                path: path.to_string(),
                operation: "read".to_string(),
            }),
            _ => Err(FsError::NotFound {
                path: path.to_string(),
                operation: "open".to_string(),
            }),
        }
    }

    async fn write_file(&self, path: &str, content: &[u8]) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let normalized = normalize_path(path);
        ensure_parent_dirs(&mut data, &normalized);
        data.insert(normalized, FsEntry::File {
            content: content.to_vec(),
            mode: 0o644,
            mtime: SystemTime::now(),
        });
        Ok(())
    }

    async fn append_file(&self, path: &str, content: &[u8]) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let normalized = normalize_path(path);

        if let Some(FsEntry::Directory { .. }) = data.get(&normalized) {
            return Err(FsError::IsDirectory {
                path: path.to_string(),
                operation: "write".to_string(),
            });
        }

        if let Some(FsEntry::File { content: existing, mode, .. }) = data.get(&normalized) {
            let mut combined = existing.clone();
            let mode = *mode;
            combined.extend_from_slice(content);
            data.insert(normalized, FsEntry::File {
                content: combined,
                mode,
                mtime: SystemTime::now(),
            });
        } else {
            ensure_parent_dirs(&mut data, &normalized);
            data.insert(normalized, FsEntry::File {
                content: content.to_vec(),
                mode: 0o644,
                mtime: SystemTime::now(),
            });
        }
        Ok(())
    }

    async fn exists(&self, path: &str) -> bool {
        let data = self.data.read().await;
        match resolve_path_with_symlinks(&data, path, "access") {
            Ok(resolved) => data.contains_key(&resolved),
            Err(_) => false,
        }
    }

    async fn stat(&self, path: &str) -> Result<FsStat, FsError> {
        let data = self.data.read().await;
        let resolved = resolve_path_with_symlinks(&data, path, "stat")?;
        match data.get(&resolved) {
            Some(entry) => {
                let size = if let FsEntry::File { content, .. } = entry {
                    content.len() as u64
                } else {
                    0
                };
                Ok(FsStat {
                    is_file: entry.is_file(),
                    is_directory: entry.is_directory(),
                    is_symlink: false, // stat follows symlinks
                    mode: entry.mode(),
                    size,
                    mtime: entry.mtime(),
                })
            }
            None => Err(FsError::NotFound {
                path: path.to_string(),
                operation: "stat".to_string(),
            }),
        }
    }

    async fn lstat(&self, path: &str) -> Result<FsStat, FsError> {
        let data = self.data.read().await;
        let resolved = resolve_intermediate_symlinks(&data, path, "lstat")?;
        match data.get(&resolved) {
            Some(FsEntry::Symlink { target, mode, mtime }) => Ok(FsStat {
                is_file: false,
                is_directory: false,
                is_symlink: true,
                mode: *mode,
                size: target.len() as u64,
                mtime: *mtime,
            }),
            Some(entry) => {
                let size = if let FsEntry::File { content, .. } = entry {
                    content.len() as u64
                } else {
                    0
                };
                Ok(FsStat {
                    is_file: entry.is_file(),
                    is_directory: entry.is_directory(),
                    is_symlink: false,
                    mode: entry.mode(),
                    size,
                    mtime: entry.mtime(),
                })
            }
            None => Err(FsError::NotFound {
                path: path.to_string(),
                operation: "lstat".to_string(),
            }),
        }
    }

    async fn mkdir(&self, path: &str, options: &MkdirOptions) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let normalized = normalize_path(path);

        if data.contains_key(&normalized) {
            if let Some(FsEntry::File { .. }) = data.get(&normalized) {
                return Err(FsError::AlreadyExists {
                    path: path.to_string(),
                    operation: "mkdir".to_string(),
                });
            }
            if !options.recursive {
                return Err(FsError::AlreadyExists {
                    path: path.to_string(),
                    operation: "mkdir".to_string(),
                });
            }
            return Ok(());
        }

        let parent = dirname(&normalized);
        if parent != "/" && !data.contains_key(&parent) {
            if options.recursive {
                // Recursively create parents
                let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();
                let mut current = String::new();
                for part in parts {
                    current = format!("{}/{}", current, part);
                    if !data.contains_key(&current) {
                        data.insert(current.clone(), FsEntry::Directory {
                            mode: 0o755,
                            mtime: SystemTime::now(),
                        });
                    }
                }
                return Ok(());
            } else {
                return Err(FsError::NotFound {
                    path: path.to_string(),
                    operation: "mkdir".to_string(),
                });
            }
        }

        data.insert(normalized, FsEntry::Directory {
            mode: 0o755,
            mtime: SystemTime::now(),
        });
        Ok(())
    }

    async fn readdir(&self, path: &str) -> Result<Vec<String>, FsError> {
        let entries = self.readdir_with_file_types(path).await?;
        Ok(entries.into_iter().map(|e| e.name).collect())
    }

    async fn readdir_with_file_types(&self, path: &str) -> Result<Vec<DirentEntry>, FsError> {
        let data = self.data.read().await;
        let mut normalized = normalize_path(path);

        // Follow symlinks on the directory itself
        let mut seen = HashSet::new();
        loop {
            match data.get(&normalized) {
                Some(FsEntry::Symlink { target, .. }) => {
                    if seen.contains(&normalized) {
                        return Err(FsError::SymlinkLoop {
                            path: path.to_string(),
                            operation: "scandir".to_string(),
                        });
                    }
                    seen.insert(normalized.clone());
                    normalized = resolve_symlink_target(&normalized, target);
                }
                Some(FsEntry::Directory { .. }) => break,
                Some(_) => return Err(FsError::NotDirectory {
                    path: path.to_string(),
                    operation: "scandir".to_string(),
                }),
                None => return Err(FsError::NotFound {
                    path: path.to_string(),
                    operation: "scandir".to_string(),
                }),
            }
        }

        let prefix = if normalized == "/" {
            "/".to_string()
        } else {
            format!("{}/", normalized)
        };

        let mut entries_map: HashMap<String, DirentEntry> = HashMap::new();
        for (p, fs_entry) in data.iter() {
            if p == &normalized {
                continue;
            }
            if let Some(rest) = p.strip_prefix(&prefix) {
                let name = rest.split('/').next().unwrap_or("");
                if !name.is_empty() && !rest[name.len()..].contains('/') && !entries_map.contains_key(name) {
                    entries_map.insert(name.to_string(), DirentEntry {
                        name: name.to_string(),
                        is_file: fs_entry.is_file(),
                        is_directory: fs_entry.is_directory(),
                        is_symlink: fs_entry.is_symlink(),
                    });
                }
            }
        }

        let mut entries: Vec<DirentEntry> = entries_map.into_values().collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    async fn rm(&self, path: &str, options: &RmOptions) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let normalized = normalize_path(path);

        if !data.contains_key(&normalized) {
            if options.force {
                return Ok(());
            }
            return Err(FsError::NotFound {
                path: path.to_string(),
                operation: "rm".to_string(),
            });
        }

        if let Some(FsEntry::Directory { .. }) = data.get(&normalized) {
            // Collect children
            let prefix = if normalized == "/" {
                "/".to_string()
            } else {
                format!("{}/", normalized)
            };
            let children: Vec<String> = data.keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect();

            if !children.is_empty() && !options.recursive {
                return Err(FsError::NotEmpty {
                    path: path.to_string(),
                    operation: "rm".to_string(),
                });
            }
            // Remove all children
            for child in children {
                data.remove(&child);
            }
        }

        data.remove(&normalized);
        Ok(())
    }

    async fn cp(&self, src: &str, dest: &str, options: &CpOptions) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let src_norm = normalize_path(src);
        let dest_norm = normalize_path(dest);

        let src_entry = data.get(&src_norm).cloned();
        match src_entry {
            None => Err(FsError::NotFound {
                path: src.to_string(),
                operation: "cp".to_string(),
            }),
            Some(FsEntry::File { content, mode, mtime }) => {
                ensure_parent_dirs(&mut data, &dest_norm);
                data.insert(dest_norm, FsEntry::File { content, mode, mtime });
                Ok(())
            }
            Some(FsEntry::Directory { .. }) => {
                if !options.recursive {
                    return Err(FsError::IsDirectory {
                        path: src.to_string(),
                        operation: "cp".to_string(),
                    });
                }
                // Collect all entries under src
                let prefix = if src_norm == "/" {
                    "/".to_string()
                } else {
                    format!("{}/", src_norm)
                };
                let entries: Vec<(String, FsEntry)> = data.iter()
                    .filter(|(k, _)| k.starts_with(&prefix) || *k == &src_norm)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                for (k, v) in entries {
                    let relative = if k == src_norm {
                        String::new()
                    } else {
                        k[src_norm.len()..].to_string()
                    };
                    let new_path = format!("{}{}", dest_norm, relative);
                    ensure_parent_dirs(&mut data, &new_path);
                    data.insert(new_path, v);
                }
                Ok(())
            }
            Some(FsEntry::Symlink { target, mode, mtime }) => {
                ensure_parent_dirs(&mut data, &dest_norm);
                data.insert(dest_norm, FsEntry::Symlink { target, mode, mtime });
                Ok(())
            }
        }
    }

    async fn mv(&self, src: &str, dest: &str) -> Result<(), FsError> {
        self.cp(src, dest, &CpOptions { recursive: true }).await?;
        self.rm(src, &RmOptions { recursive: true, force: false }).await
    }

    async fn chmod(&self, path: &str, mode: u32) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let normalized = normalize_path(path);
        match data.get_mut(&normalized) {
            Some(FsEntry::File { mode: m, .. }) => { *m = mode; Ok(()) }
            Some(FsEntry::Directory { mode: m, .. }) => { *m = mode; Ok(()) }
            Some(FsEntry::Symlink { mode: m, .. }) => { *m = mode; Ok(()) }
            None => Err(FsError::NotFound {
                path: path.to_string(),
                operation: "chmod".to_string(),
            }),
        }
    }

    async fn symlink(&self, target: &str, link_path: &str) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let normalized = normalize_path(link_path);
        if data.contains_key(&normalized) {
            return Err(FsError::AlreadyExists {
                path: link_path.to_string(),
                operation: "symlink".to_string(),
            });
        }
        ensure_parent_dirs(&mut data, &normalized);
        data.insert(normalized, FsEntry::Symlink {
            target: target.to_string(),
            mode: 0o777,
            mtime: SystemTime::now(),
        });
        Ok(())
    }

    async fn link(&self, existing_path: &str, new_path: &str) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let existing_norm = normalize_path(existing_path);
        let new_norm = normalize_path(new_path);

        let entry = data.get(&existing_norm).cloned();
        match entry {
            None => Err(FsError::NotFound {
                path: existing_path.to_string(),
                operation: "link".to_string(),
            }),
            Some(FsEntry::File { content, mode, mtime }) => {
                if data.contains_key(&new_norm) {
                    return Err(FsError::AlreadyExists {
                        path: new_path.to_string(),
                        operation: "link".to_string(),
                    });
                }
                ensure_parent_dirs(&mut data, &new_norm);
                data.insert(new_norm, FsEntry::File { content, mode, mtime });
                Ok(())
            }
            _ => Err(FsError::PermissionDenied {
                path: existing_path.to_string(),
                operation: "link".to_string(),
            }),
        }
    }

    async fn readlink(&self, path: &str) -> Result<String, FsError> {
        let data = self.data.read().await;
        let normalized = normalize_path(path);
        match data.get(&normalized) {
            Some(FsEntry::Symlink { target, .. }) => Ok(target.clone()),
            Some(_) => Err(FsError::InvalidArgument {
                path: path.to_string(),
                operation: "readlink".to_string(),
            }),
            None => Err(FsError::NotFound {
                path: path.to_string(),
                operation: "readlink".to_string(),
            }),
        }
    }

    async fn realpath(&self, path: &str) -> Result<String, FsError> {
        let data = self.data.read().await;
        let resolved = resolve_path_with_symlinks(&data, path, "realpath")?;
        if !data.contains_key(&resolved) {
            return Err(FsError::NotFound {
                path: path.to_string(),
                operation: "realpath".to_string(),
            });
        }
        Ok(resolved)
    }

    async fn utimes(&self, path: &str, mtime: SystemTime) -> Result<(), FsError> {
        let mut data = self.data.write().await;
        let resolved = resolve_path_with_symlinks(&data, path, "utimes")?;
        match data.get_mut(&resolved) {
            Some(FsEntry::File { mtime: m, .. }) => { *m = mtime; Ok(()) }
            Some(FsEntry::Directory { mtime: m, .. }) => { *m = mtime; Ok(()) }
            Some(FsEntry::Symlink { mtime: m, .. }) => { *m = mtime; Ok(()) }
            None => Err(FsError::NotFound {
                path: path.to_string(),
                operation: "utimes".to_string(),
            }),
        }
    }

    fn resolve_path(&self, base: &str, path: &str) -> String {
        if path.starts_with('/') {
            normalize_path(path)
        } else if base == "/" {
            normalize_path(&format!("/{}", path))
        } else {
            normalize_path(&format!("{}/{}", base, path))
        }
    }

    fn get_all_paths(&self) -> Vec<String> {
        // Use try_read first, fall back to block_in_place + blocking_read
        match self.data.try_read() {
            Ok(data) => data.keys().cloned().collect(),
            Err(_) => {
                // If we can't get the lock immediately, use block_in_place
                tokio::task::block_in_place(|| {
                    let data = self.data.blocking_read();
                    data.keys().cloned().collect()
                })
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path(""), "/");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path("/foo/bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/bar/"), "/foo/bar");
        assert_eq!(normalize_path("foo/bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/./bar"), "/foo/bar");
        assert_eq!(normalize_path("/foo/../bar"), "/bar");
        assert_eq!(normalize_path("/foo/bar/.."), "/foo");
        assert_eq!(normalize_path("/../.."), "/");
    }

    #[test]
    fn test_dirname_fn() {
        assert_eq!(dirname("/"), "/");
        assert_eq!(dirname("/foo"), "/");
        assert_eq!(dirname("/foo/bar"), "/foo");
        assert_eq!(dirname("/foo/bar/baz"), "/foo/bar");
    }

    #[tokio::test]
    async fn test_basic_file_ops() {
        let fs = InMemoryFs::new();
        fs.write_file("/test.txt", b"hello").await.unwrap();
        assert!(fs.exists("/test.txt").await);
        let content = fs.read_file("/test.txt").await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_mkdir_and_readdir() {
        let fs = InMemoryFs::new();
        fs.mkdir("/foo", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/foo/a.txt", b"a").await.unwrap();
        fs.write_file("/foo/b.txt", b"b").await.unwrap();
        let entries = fs.readdir("/foo").await.unwrap();
        assert_eq!(entries, vec!["a.txt", "b.txt"]);
    }

    #[tokio::test]
    async fn test_mkdir_recursive() {
        let fs = InMemoryFs::new();
        fs.mkdir("/a/b/c", &MkdirOptions { recursive: true }).await.unwrap();
        assert!(fs.exists("/a").await);
        assert!(fs.exists("/a/b").await);
        assert!(fs.exists("/a/b/c").await);
    }

    #[tokio::test]
    async fn test_rm_recursive() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/dir/file.txt", b"data").await.unwrap();
        fs.rm("/dir", &RmOptions { recursive: true, force: false }).await.unwrap();
        assert!(!fs.exists("/dir").await);
        assert!(!fs.exists("/dir/file.txt").await);
    }

    #[tokio::test]
    async fn test_symlink_and_readlink() {
        let fs = InMemoryFs::new();
        fs.write_file("/target.txt", b"content").await.unwrap();
        fs.symlink("/target.txt", "/link.txt").await.unwrap();
        let target = fs.readlink("/link.txt").await.unwrap();
        assert_eq!(target, "/target.txt");
        let content = fs.read_file("/link.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_stat_and_lstat() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"hello").await.unwrap();
        fs.symlink("/file.txt", "/link.txt").await.unwrap();

        let stat = fs.stat("/link.txt").await.unwrap();
        assert!(stat.is_file); // stat follows symlinks
        assert!(!stat.is_symlink);

        let lstat = fs.lstat("/link.txt").await.unwrap();
        assert!(lstat.is_symlink); // lstat does not follow
        assert!(!lstat.is_file);
    }

    #[tokio::test]
    async fn test_append_file() {
        let fs = InMemoryFs::new();
        fs.write_file("/f.txt", b"hello").await.unwrap();
        fs.append_file("/f.txt", b" world").await.unwrap();
        let content = fs.read_file("/f.txt").await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_cp_and_mv() {
        let fs = InMemoryFs::new();
        fs.write_file("/src.txt", b"data").await.unwrap();
        fs.cp("/src.txt", "/dst.txt", &CpOptions { recursive: false }).await.unwrap();
        assert!(fs.exists("/dst.txt").await);
        assert!(fs.exists("/src.txt").await);

        fs.mv("/dst.txt", "/moved.txt").await.unwrap();
        assert!(fs.exists("/moved.txt").await);
        assert!(!fs.exists("/dst.txt").await);
    }

    #[tokio::test]
    async fn test_realpath() {
        let fs = InMemoryFs::new();
        fs.mkdir("/a", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/a/file.txt", b"x").await.unwrap();
        fs.symlink("/a", "/link").await.unwrap();
        let real = fs.realpath("/link/file.txt").await.unwrap();
        assert_eq!(real, "/a/file.txt");
    }

    // ============================================================================
    // Additional comprehensive tests
    // ============================================================================

    #[tokio::test]
    async fn test_write_and_read_binary_data() {
        let fs = InMemoryFs::new();
        let data = vec![0x00, 0x01, 0x02, 0xff, 0xfe];
        fs.write_file("/binary.bin", &data).await.unwrap();
        let result = fs.read_file_buffer("/binary.bin").await.unwrap();
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn test_write_and_read_empty_file() {
        let fs = InMemoryFs::new();
        fs.write_file("/empty.txt", b"").await.unwrap();
        assert!(fs.exists("/empty.txt").await);
        let content = fs.read_file("/empty.txt").await.unwrap();
        assert_eq!(content, "");
        let stat = fs.stat("/empty.txt").await.unwrap();
        assert_eq!(stat.size, 0);
    }

    #[tokio::test]
    async fn test_read_file_buffer_with_null_bytes() {
        let fs = InMemoryFs::new();
        let data = vec![0x00, 0x01, 0x00, 0xff, 0x00];
        fs.write_file("/nulls.bin", &data).await.unwrap();
        let result = fs.read_file_buffer("/nulls.bin").await.unwrap();
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn test_append_to_nonexistent_file() {
        let fs = InMemoryFs::new();
        fs.append_file("/new.txt", b"hello").await.unwrap();
        let content = fs.read_file("/new.txt").await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_append_binary_data() {
        let fs = InMemoryFs::new();
        fs.write_file("/bin.dat", &[0x01, 0x02]).await.unwrap();
        fs.append_file("/bin.dat", &[0x03, 0x04]).await.unwrap();
        let result = fs.read_file_buffer("/bin.dat").await.unwrap();
        assert_eq!(result, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[tokio::test]
    async fn test_append_to_directory_fails() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        let result = fs.append_file("/dir", b"data").await;
        assert!(result.is_err());
        match result {
            Err(FsError::IsDirectory { .. }) => {},
            _ => panic!("Expected IsDirectory error"),
        }
    }

    #[tokio::test]
    async fn test_mkdir_already_exists_error() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        let result = fs.mkdir("/dir", &MkdirOptions { recursive: false }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mkdir_recursive_idempotent() {
        let fs = InMemoryFs::new();
        fs.mkdir("/a/b/c", &MkdirOptions { recursive: true }).await.unwrap();
        // Should not error when called again with recursive
        fs.mkdir("/a/b/c", &MkdirOptions { recursive: true }).await.unwrap();
        assert!(fs.exists("/a/b/c").await);
    }

    #[tokio::test]
    async fn test_mkdir_parent_not_found() {
        let fs = InMemoryFs::new();
        let result = fs.mkdir("/nonexistent/child", &MkdirOptions { recursive: false }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_readdir_empty_directory() {
        let fs = InMemoryFs::new();
        fs.mkdir("/empty", &MkdirOptions { recursive: false }).await.unwrap();
        let entries = fs.readdir("/empty").await.unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn test_readdir_sorted() {
        let fs = InMemoryFs::new();
        fs.write_file("/dir/zebra.txt", b"z").await.unwrap();
        fs.write_file("/dir/apple.txt", b"a").await.unwrap();
        fs.write_file("/dir/banana.txt", b"b").await.unwrap();
        let entries = fs.readdir("/dir").await.unwrap();
        assert_eq!(entries, vec!["apple.txt", "banana.txt", "zebra.txt"]);
    }

    #[tokio::test]
    async fn test_readdir_with_file_types() {
        let fs = InMemoryFs::new();
        fs.write_file("/dir/file.txt", b"content").await.unwrap();
        fs.mkdir("/dir/subdir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.symlink("/dir/file.txt", "/dir/link.txt").await.unwrap();

        let entries = fs.readdir_with_file_types("/dir").await.unwrap();
        assert_eq!(entries.len(), 3);

        let file = entries.iter().find(|e| e.name == "file.txt").unwrap();
        assert!(file.is_file);
        assert!(!file.is_directory);
        assert!(!file.is_symlink);

        let dir = entries.iter().find(|e| e.name == "subdir").unwrap();
        assert!(!dir.is_file);
        assert!(dir.is_directory);
        assert!(!dir.is_symlink);

        let link = entries.iter().find(|e| e.name == "link.txt").unwrap();
        assert!(!link.is_file);
        assert!(!link.is_directory);
        assert!(link.is_symlink);
    }

    #[tokio::test]
    async fn test_readdir_not_directory() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"content").await.unwrap();
        let result = fs.readdir("/file.txt").await;
        assert!(result.is_err());
        match result {
            Err(FsError::NotDirectory { .. }) => {},
            _ => panic!("Expected NotDirectory error"),
        }
    }

    #[tokio::test]
    async fn test_rm_nonexistent_with_force() {
        let fs = InMemoryFs::new();
        let result = fs.rm("/nonexistent", &RmOptions { recursive: false, force: true }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_rm_nonexistent_without_force() {
        let fs = InMemoryFs::new();
        let result = fs.rm("/nonexistent", &RmOptions { recursive: false, force: false }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rm_directory_not_empty() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/dir/file.txt", b"data").await.unwrap();
        let result = fs.rm("/dir", &RmOptions { recursive: false, force: false }).await;
        assert!(result.is_err());
        match result {
            Err(FsError::NotEmpty { .. }) => {},
            _ => panic!("Expected NotEmpty error"),
        }
    }

    #[tokio::test]
    async fn test_rm_file() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"data").await.unwrap();
        fs.rm("/file.txt", &RmOptions { recursive: false, force: false }).await.unwrap();
        assert!(!fs.exists("/file.txt").await);
    }

    #[tokio::test]
    async fn test_cp_file_to_file() {
        let fs = InMemoryFs::new();
        fs.write_file("/src.txt", b"content").await.unwrap();
        fs.cp("/src.txt", "/dst.txt", &CpOptions { recursive: false }).await.unwrap();
        let content = fs.read_file("/dst.txt").await.unwrap();
        assert_eq!(content, "content");
        assert!(fs.exists("/src.txt").await);
    }

    #[tokio::test]
    async fn test_cp_directory_without_recursive() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        let result = fs.cp("/dir", "/dst", &CpOptions { recursive: false }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_cp_directory_recursive() {
        let fs = InMemoryFs::new();
        fs.mkdir("/src/sub", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/src/file.txt", b"data").await.unwrap();
        fs.write_file("/src/sub/nested.txt", b"nested").await.unwrap();

        fs.cp("/src", "/dst", &CpOptions { recursive: true }).await.unwrap();

        assert!(fs.exists("/dst").await);
        assert!(fs.exists("/dst/file.txt").await);
        assert!(fs.exists("/dst/sub/nested.txt").await);
        let content = fs.read_file("/dst/sub/nested.txt").await.unwrap();
        assert_eq!(content, "nested");
    }

    #[tokio::test]
    async fn test_cp_preserves_binary_content() {
        let fs = InMemoryFs::new();
        let data = vec![0x00, 0xff, 0x00, 0xff];
        fs.write_file("/src.bin", &data).await.unwrap();
        fs.cp("/src.bin", "/dst.bin", &CpOptions { recursive: false }).await.unwrap();
        let result = fs.read_file_buffer("/dst.bin").await.unwrap();
        assert_eq!(result, data);
    }

    #[tokio::test]
    async fn test_mv_file() {
        let fs = InMemoryFs::new();
        fs.write_file("/src.txt", b"data").await.unwrap();
        fs.mv("/src.txt", "/dst.txt").await.unwrap();
        assert!(!fs.exists("/src.txt").await);
        assert!(fs.exists("/dst.txt").await);
        let content = fs.read_file("/dst.txt").await.unwrap();
        assert_eq!(content, "data");
    }

    #[tokio::test]
    async fn test_mv_directory() {
        let fs = InMemoryFs::new();
        fs.mkdir("/src", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/src/file.txt", b"data").await.unwrap();
        fs.mv("/src", "/dst").await.unwrap();
        assert!(!fs.exists("/src").await);
        assert!(fs.exists("/dst/file.txt").await);
    }

    #[tokio::test]
    async fn test_chmod_file() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"data").await.unwrap();
        fs.chmod("/file.txt", 0o600).await.unwrap();
        let stat = fs.stat("/file.txt").await.unwrap();
        assert_eq!(stat.mode, 0o600);
    }

    #[tokio::test]
    async fn test_chmod_directory() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.chmod("/dir", 0o700).await.unwrap();
        let stat = fs.stat("/dir").await.unwrap();
        assert_eq!(stat.mode, 0o700);
    }

    #[tokio::test]
    async fn test_chmod_nonexistent() {
        let fs = InMemoryFs::new();
        let result = fs.chmod("/nonexistent", 0o644).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_symlink_relative_target() {
        let fs = InMemoryFs::new();
        fs.write_file("/dir/target.txt", b"content").await.unwrap();
        fs.symlink("target.txt", "/dir/link.txt").await.unwrap();
        let target = fs.readlink("/dir/link.txt").await.unwrap();
        assert_eq!(target, "target.txt");
        let content = fs.read_file("/dir/link.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_symlink_absolute_target() {
        let fs = InMemoryFs::new();
        fs.write_file("/target.txt", b"content").await.unwrap();
        fs.symlink("/target.txt", "/link.txt").await.unwrap();
        let target = fs.readlink("/link.txt").await.unwrap();
        assert_eq!(target, "/target.txt");
    }

    #[tokio::test]
    async fn test_symlink_already_exists() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"data").await.unwrap();
        let result = fs.symlink("/target", "/file.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_symlink_chain() {
        let fs = InMemoryFs::new();
        fs.write_file("/target.txt", b"content").await.unwrap();
        fs.symlink("/target.txt", "/link1.txt").await.unwrap();
        fs.symlink("/link1.txt", "/link2.txt").await.unwrap();
        let content = fs.read_file("/link2.txt").await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_link_creates_hard_link() {
        let fs = InMemoryFs::new();
        fs.write_file("/original.txt", b"data").await.unwrap();
        fs.link("/original.txt", "/hardlink.txt").await.unwrap();

        // Both should exist
        assert!(fs.exists("/original.txt").await);
        assert!(fs.exists("/hardlink.txt").await);

        // Content should be the same
        let content = fs.read_file("/hardlink.txt").await.unwrap();
        assert_eq!(content, "data");
    }

    #[tokio::test]
    async fn test_link_nonexistent_source() {
        let fs = InMemoryFs::new();
        let result = fs.link("/nonexistent", "/link").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_link_target_exists() {
        let fs = InMemoryFs::new();
        fs.write_file("/src.txt", b"data").await.unwrap();
        fs.write_file("/dst.txt", b"other").await.unwrap();
        let result = fs.link("/src.txt", "/dst.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_readlink_not_symlink() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"data").await.unwrap();
        let result = fs.readlink("/file.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_readlink_nonexistent() {
        let fs = InMemoryFs::new();
        let result = fs.readlink("/nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_realpath_nonexistent() {
        let fs = InMemoryFs::new();
        let result = fs.realpath("/nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_realpath_with_dotdot() {
        let fs = InMemoryFs::new();
        fs.mkdir("/a/b", &MkdirOptions { recursive: true }).await.unwrap();
        fs.write_file("/a/file.txt", b"data").await.unwrap();
        let real = fs.realpath("/a/b/../file.txt").await.unwrap();
        assert_eq!(real, "/a/file.txt");
    }

    #[tokio::test]
    async fn test_stat_file_size() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"hello world").await.unwrap();
        let stat = fs.stat("/file.txt").await.unwrap();
        assert_eq!(stat.size, 11);
        assert!(stat.is_file);
        assert!(!stat.is_directory);
    }

    #[tokio::test]
    async fn test_stat_directory() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        let stat = fs.stat("/dir").await.unwrap();
        assert!(!stat.is_file);
        assert!(stat.is_directory);
        assert_eq!(stat.size, 0);
    }

    #[tokio::test]
    async fn test_stat_follows_symlinks() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"data").await.unwrap();
        fs.symlink("/file.txt", "/link.txt").await.unwrap();
        let stat = fs.stat("/link.txt").await.unwrap();
        assert!(stat.is_file);
        assert!(!stat.is_symlink);
    }

    #[tokio::test]
    async fn test_lstat_does_not_follow_symlinks() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"data").await.unwrap();
        fs.symlink("/file.txt", "/link.txt").await.unwrap();
        let lstat = fs.lstat("/link.txt").await.unwrap();
        assert!(lstat.is_symlink);
        assert!(!lstat.is_file);
    }

    #[tokio::test]
    async fn test_utimes_file() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"data").await.unwrap();
        let new_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000000);
        fs.utimes("/file.txt", new_time).await.unwrap();
        let stat = fs.stat("/file.txt").await.unwrap();
        assert_eq!(stat.mtime, new_time);
    }

    #[tokio::test]
    async fn test_utimes_directory() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        let new_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2000000);
        fs.utimes("/dir", new_time).await.unwrap();
        let stat = fs.stat("/dir").await.unwrap();
        assert_eq!(stat.mtime, new_time);
    }

    #[tokio::test]
    async fn test_exists_returns_false_for_nonexistent() {
        let fs = InMemoryFs::new();
        assert!(!fs.exists("/nonexistent").await);
    }

    #[tokio::test]
    async fn test_exists_returns_true_for_file() {
        let fs = InMemoryFs::new();
        fs.write_file("/file.txt", b"data").await.unwrap();
        assert!(fs.exists("/file.txt").await);
    }

    #[tokio::test]
    async fn test_exists_returns_true_for_directory() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        assert!(fs.exists("/dir").await);
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let fs = InMemoryFs::new();
        let result = fs.read_file("/nonexistent.txt").await;
        assert!(result.is_err());
        match result {
            Err(FsError::NotFound { .. }) => {},
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_read_file_is_directory() {
        let fs = InMemoryFs::new();
        fs.mkdir("/dir", &MkdirOptions { recursive: false }).await.unwrap();
        let result = fs.read_file("/dir").await;
        assert!(result.is_err());
        match result {
            Err(FsError::IsDirectory { .. }) => {},
            _ => panic!("Expected IsDirectory error"),
        }
    }

    #[tokio::test]
    async fn test_with_files_constructor() {
        let mut files = HashMap::new();
        files.insert("/file1.txt".to_string(), FileInit {
            content: FileContent::Text("hello".to_string()),
            mode: Some(0o644),
            mtime: None,
        });
        files.insert("/file2.txt".to_string(), FileInit {
            content: FileContent::Binary(vec![0x01, 0x02, 0x03]),
            mode: Some(0o600),
            mtime: None,
        });

        // with_files uses blocking operations, so we need to call it outside the async context
        let fs = tokio::task::spawn_blocking(move || {
            InMemoryFs::with_files(&files)
        }).await.unwrap();

        let content1 = fs.read_file("/file1.txt").await.unwrap();
        assert_eq!(content1, "hello");

        let content2 = fs.read_file_buffer("/file2.txt").await.unwrap();
        assert_eq!(content2, vec![0x01, 0x02, 0x03]);

        let stat = fs.stat("/file2.txt").await.unwrap();
        assert_eq!(stat.mode, 0o600);
    }

    #[tokio::test]
    async fn test_large_file() {
        let fs = InMemoryFs::new();
        let size = 1024 * 1024; // 1MB
        let mut data = vec![0u8; size];
        for i in 0..size {
            data[i] = (i % 256) as u8;
        }

        fs.write_file("/large.bin", &data).await.unwrap();
        let result = fs.read_file_buffer("/large.bin").await.unwrap();

        assert_eq!(result.len(), size);
        assert_eq!(result[0], 0);
        assert_eq!(result[255], 255);
        assert_eq!(result[256], 0);
    }

    #[tokio::test]
    async fn test_cp_symlink() {
        let fs = InMemoryFs::new();
        fs.write_file("/target.txt", b"data").await.unwrap();
        fs.symlink("/target.txt", "/link.txt").await.unwrap();
        fs.cp("/link.txt", "/copy.txt", &CpOptions { recursive: false }).await.unwrap();

        // The copy should also be a symlink
        let lstat = fs.lstat("/copy.txt").await.unwrap();
        assert!(lstat.is_symlink);
    }

    #[tokio::test]
    async fn test_readdir_follows_symlink_to_directory() {
        let fs = InMemoryFs::new();
        fs.mkdir("/realdir", &MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/realdir/file.txt", b"data").await.unwrap();
        fs.symlink("/realdir", "/linkdir").await.unwrap();

        let entries = fs.readdir("/linkdir").await.unwrap();
        assert_eq!(entries, vec!["file.txt"]);
    }
}
