//! Sync FileSystem Adapter
//!
//! Bridges the async `fs::FileSystem` trait to the sync `interpreter::FileSystem` trait.
//! Uses `tokio::task::block_in_place` + `block_on` to execute async operations synchronously.

use std::sync::Arc;
use crate::fs::FileSystem as AsyncFileSystem;
use crate::interpreter::interpreter::{FileSystem as SyncFileSystem, FileStat};

/// Adapter that wraps an async FileSystem and provides a sync interface.
///
/// This is used by the execution engine to bridge the async filesystem
/// with the sync interpreter helper functions.
pub struct SyncFsAdapter {
    inner: Arc<dyn AsyncFileSystem>,
    handle: tokio::runtime::Handle,
}

impl SyncFsAdapter {
    /// Create a new adapter wrapping the given async filesystem.
    ///
    /// # Arguments
    /// * `fs` - The async filesystem to wrap
    /// * `handle` - The tokio runtime handle for executing async operations
    pub fn new(fs: Arc<dyn AsyncFileSystem>, handle: tokio::runtime::Handle) -> Self {
        Self { inner: fs, handle }
    }

    /// Execute an async operation synchronously using block_in_place.
    fn block_on<F, T>(&self, f: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        tokio::task::block_in_place(|| self.handle.block_on(f))
    }
}

impl SyncFileSystem for SyncFsAdapter {
    fn read_file(&self, path: &str) -> Result<String, std::io::Error> {
        self.block_on(self.inner.read_file(path))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }

    fn write_file(&self, path: &str, contents: &str) -> Result<(), std::io::Error> {
        self.block_on(self.inner.write_file(path, contents.as_bytes()))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }

    fn append_file(&self, path: &str, contents: &str) -> Result<(), std::io::Error> {
        self.block_on(self.inner.append_file(path, contents.as_bytes()))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }

    fn exists(&self, path: &str) -> bool {
        self.block_on(self.inner.exists(path))
    }

    fn is_dir(&self, path: &str) -> bool {
        self.block_on(self.inner.stat(path))
            .map(|s| s.is_directory)
            .unwrap_or(false)
    }

    fn is_file(&self, path: &str) -> bool {
        self.block_on(self.inner.stat(path))
            .map(|s| s.is_file)
            .unwrap_or(false)
    }

    fn resolve_path(&self, base: &str, path: &str) -> String {
        self.inner.resolve_path(base, path)
    }

    fn stat(&self, path: &str) -> Result<FileStat, std::io::Error> {
        let s = self.block_on(self.inner.stat(path))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(FileStat {
            is_file: s.is_file,
            is_dir: s.is_directory,
            is_symlink: s.is_symlink,
            size: s.size,
            mode: s.mode,
            uid: 0,  // Not tracked in our virtual FS
            gid: 0,  // Not tracked in our virtual FS
            mtime: s.mtime.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_secs(),
        })
    }

    fn read_dir(&self, path: &str) -> Result<Vec<String>, std::io::Error> {
        self.block_on(self.inner.readdir(path))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }

    fn glob(&self, pattern: &str, cwd: &str) -> Result<Vec<String>, std::io::Error> {
        // Get all paths from the filesystem
        let all_paths = self.inner.get_all_paths();

        // Use the glob crate to match patterns
        let glob_pattern = if pattern.starts_with('/') {
            pattern.to_string()
        } else {
            // Relative pattern - prepend cwd
            if cwd == "/" {
                format!("/{}", pattern)
            } else {
                format!("{}/{}", cwd, pattern)
            }
        };

        // Compile the glob pattern
        let matcher = match glob::Pattern::new(&glob_pattern) {
            Ok(m) => m,
            Err(e) => return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Invalid glob pattern: {}", e),
            )),
        };

        // Filter paths that match the pattern
        let matches: Vec<String> = all_paths
            .into_iter()
            .filter(|p| matcher.matches(p))
            .collect();

        Ok(matches)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::InMemoryFs;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_fs_adapter_read_write() {
        let fs = Arc::new(InMemoryFs::new());
        let handle = tokio::runtime::Handle::current();

        // Write using async API
        fs.write_file("/test.txt", b"hello").await.unwrap();

        // Read using sync adapter
        let adapter = SyncFsAdapter::new(fs.clone(), handle);
        let content = adapter.read_file("/test.txt").unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_fs_adapter_exists() {
        let fs = Arc::new(InMemoryFs::new());
        let handle = tokio::runtime::Handle::current();

        fs.write_file("/exists.txt", b"data").await.unwrap();

        let adapter = SyncFsAdapter::new(fs, handle);
        assert!(adapter.exists("/exists.txt"));
        assert!(!adapter.exists("/not_exists.txt"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_fs_adapter_is_dir_is_file() {
        let fs = Arc::new(InMemoryFs::new());
        let handle = tokio::runtime::Handle::current();

        fs.mkdir("/mydir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/myfile.txt", b"data").await.unwrap();

        let adapter = SyncFsAdapter::new(fs, handle);
        assert!(adapter.is_dir("/mydir"));
        assert!(!adapter.is_file("/mydir"));
        assert!(adapter.is_file("/myfile.txt"));
        assert!(!adapter.is_dir("/myfile.txt"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_fs_adapter_stat() {
        let fs = Arc::new(InMemoryFs::new());
        let handle = tokio::runtime::Handle::current();

        fs.write_file("/stat_test.txt", b"hello world").await.unwrap();

        let adapter = SyncFsAdapter::new(fs, handle);
        let stat = adapter.stat("/stat_test.txt").unwrap();
        assert!(stat.is_file);
        assert!(!stat.is_dir);
        assert_eq!(stat.size, 11);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_fs_adapter_read_dir() {
        let fs = Arc::new(InMemoryFs::new());
        let handle = tokio::runtime::Handle::current();

        fs.mkdir("/testdir", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/testdir/a.txt", b"a").await.unwrap();
        fs.write_file("/testdir/b.txt", b"b").await.unwrap();

        let adapter = SyncFsAdapter::new(fs, handle);
        let entries = adapter.read_dir("/testdir").unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&"a.txt".to_string()));
        assert!(entries.contains(&"b.txt".to_string()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_fs_adapter_resolve_path() {
        let fs = Arc::new(InMemoryFs::new());
        let handle = tokio::runtime::Handle::current();

        let adapter = SyncFsAdapter::new(fs, handle);
        assert_eq!(adapter.resolve_path("/home/user", "file.txt"), "/home/user/file.txt");
        assert_eq!(adapter.resolve_path("/home/user", "/absolute/path"), "/absolute/path");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_fs_adapter_glob() {
        let fs = Arc::new(InMemoryFs::new());
        let handle = tokio::runtime::Handle::current();

        fs.mkdir("/glob_test", &crate::fs::MkdirOptions { recursive: false }).await.unwrap();
        fs.write_file("/glob_test/a.txt", b"a").await.unwrap();
        fs.write_file("/glob_test/b.txt", b"b").await.unwrap();
        fs.write_file("/glob_test/c.md", b"c").await.unwrap();

        let adapter = SyncFsAdapter::new(fs, handle);
        let matches = adapter.glob("/glob_test/*.txt", "/").unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"/glob_test/a.txt".to_string()));
        assert!(matches.contains(&"/glob_test/b.txt".to_string()));
    }
}
