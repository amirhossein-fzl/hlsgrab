use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};

static REGISTRY: OnceLock<Mutex<HashMap<u64, PathBuf>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static SIGNAL_HOOK_INIT: Once = Once::new();
static PANIC_HOOK_INIT: Once = Once::new();

fn registry() -> &'static Mutex<HashMap<u64, PathBuf>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register(path: PathBuf) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let mut map = registry().lock().unwrap_or_else(|e| e.into_inner());
    map.insert(id, path);
    id
}

fn unregister(id: u64) {
    let mut map = registry().lock().unwrap_or_else(|e| e.into_inner());
    map.remove(&id);
}

fn cleanup_all() {
    let paths: Vec<PathBuf> = match registry().try_lock() {
        Ok(map) => map.values().cloned().collect(),
        Err(_) => return,
    };
    for path in paths {
        let _ = std::fs::remove_file(&path);
    }
}

pub fn force_cleanup_now() {
    cleanup_all();
}

pub fn install_panic_hook() {
    PANIC_HOOK_INIT.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            cleanup_all();
            previous(info);
        }));
    });
}

pub fn install_signal_handler() -> Result<(), ctrlc::Error> {
    let mut result = Ok(());
    SIGNAL_HOOK_INIT.call_once(|| {
        result = ctrlc::set_handler(|| {
            cleanup_all();
            std::process::exit(130);
        });
    });
    result
}

fn init() -> Result<(), ctrlc::Error> {
    install_panic_hook();
    install_signal_handler()
}

pub struct SafeTempFile {
    id: u64,
    file: Option<tempfile::NamedTempFile>,
    target: PathBuf,
}

impl SafeTempFile {
    pub fn new(target: impl AsRef<Path>) -> io::Result<Self> {
        let target = target.as_ref().to_path_buf();

        let dir = match target.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };

        let file_name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");

        let named = tempfile::Builder::new()
            .prefix(&format!(".{file_name}."))
            .suffix(".part")
            .tempfile_in(&dir)?;

        let id = register(named.path().to_path_buf());

        init().unwrap_or_else(|_| {
            println!("Wowwww");
        });

        Ok(Self {
            id,
            file: Some(named),
            target,
        })
    }

    pub fn path(&self) -> &Path {
        self.file
            .as_ref()
            .expect("SafeTempFile used after being consumed")
            .path()
    }

    pub fn target_path(&self) -> &Path {
        &self.target
    }

    pub fn as_file(&self) -> &File {
        self.file
            .as_ref()
            .expect("SafeTempFile used after being consumed")
            .as_file()
    }

    pub fn as_file_mut(&mut self) -> &mut File {
        self.file
            .as_mut()
            .expect("SafeTempFile used after being consumed")
            .as_file_mut()
    }

    pub fn commit(mut self) -> io::Result<PathBuf> {
        self.commit_inner()
    }

    pub fn commit_to(mut self, target: impl AsRef<Path>) -> io::Result<PathBuf> {
        self.target = target.as_ref().to_path_buf();
        self.commit_inner()
    }

    fn commit_inner(&mut self) -> io::Result<PathBuf> {
        let named = self
            .file
            .take()
            .expect("SafeTempFile used after being consumed");

        named.as_file().sync_all()?;

        let target = self.target.clone();
        match named.persist(&target) {
            Ok(_file) => {
                unregister(self.id);
                Ok(target)
            }
            Err(persist_err) => {
                unregister(self.id);
                Err(persist_err.error)
            }
        }
    }

    pub fn discard(mut self) {
        self.file.take();
        unregister(self.id);
    }
}

impl Write for SafeTempFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.as_file_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.as_file_mut().flush()
    }
}

impl std::fmt::Debug for SafeTempFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SafeTempFile")
            .field("path", &self.file.as_ref().map(|f| f.path()))
            .field("target", &self.target)
            .finish()
    }
}

impl Drop for SafeTempFile {
    fn drop(&mut self) {
        unregister(self.id);
    }
}


#[cfg(test)]
pub(crate) fn pending_count_for_tests() -> usize {
    registry().lock().unwrap().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn commit_writes_final_file_and_removes_temp() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");

        let mut f = SafeTempFile::new(&target).unwrap();
        let temp_path = f.path().to_path_buf();
        assert!(temp_path.exists());
        assert!(!target.exists());

        f.write_all(b"hello world").unwrap();
        let final_path = f.commit().unwrap();

        assert_eq!(final_path, target);
        assert!(target.exists());
        assert!(!temp_path.exists());

        let mut contents = String::new();
        File::open(&target)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents, "hello world");
    }

    #[test]
    fn drop_without_commit_removes_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");

        let temp_path = {
            let mut f = SafeTempFile::new(&target).unwrap();
            f.write_all(b"partial").unwrap();
            f.path().to_path_buf()
        }; // dropped here without commit

        assert!(!temp_path.exists());
        assert!(!target.exists());
    }

    #[test]
    fn error_return_path_cleans_up() {
        fn do_work(target: &Path) -> io::Result<()> {
            let mut f = SafeTempFile::new(target)?;
            f.write_all(b"partial")?;
            Err(io::Error::new(io::ErrorKind::Other, "simulated failure"))
        }

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");

        let result = do_work(&target);
        assert!(result.is_err());
        assert!(!target.exists());

        // Nothing should be left registered either.
        assert_eq!(pending_count_for_tests(), 0);
    }

    #[test]
    fn commit_overwrites_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.bin");
        std::fs::write(&target, b"old contents").unwrap();

        let mut f = SafeTempFile::new(&target).unwrap();
        f.write_all(b"new contents").unwrap();
        f.commit().unwrap();

        let contents = std::fs::read_to_string(&target).unwrap();
        assert_eq!(contents, "new contents");
    }

    #[test]
    fn init_is_idempotent() {
        // Calling multiple times must not panic, even though the
        // underlying ctrlc handler can only truly be installed once.
        let _ = init();
        let _ = init();
        install_panic_hook();
        let _ = install_signal_handler();
    }
}
