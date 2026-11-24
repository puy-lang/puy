//! A file system in memory and a fixed OS for the cases (`// puy-test: test-fs`;
//! `docs/design/testing.md` 3.2): the same results on every platform, and nothing touches the
//! disk.
//!
//! Paths use `/` only. A relative path is joined to the working directory, which `Os` changes;
//! `.` and `..` are resolved by name (there are no links). The tree starts with the working
//! directory `/work` and the directories `/tmp` and `/home/puy`. Every path has the times
//! [`MEM_TIME`] until `set_times` sets them (writing does not change them); the creation time
//! can be set, as on Windows.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use puy_host::{
    DirEntry, DirReader, File, FileTimes, FileType, Fs, HostError, HostErrorKind, HostFuture,
    Metadata, OpenOptions, Os, OsVersion, SeekFrom, ready,
};

enum Node {
    Dir,
    File(Rc<RefCell<Vec<u8>>>, bool),
}

/// The time every path has until `set_times` sets it: 2026-01-02T03:04:05Z, in nanoseconds
/// since the UNIX epoch.
pub const MEM_TIME: i64 = 1_767_323_045_000_000_000;

struct State {
    /// Every path (absolute, normalized; the root is `/`) and what it names.
    nodes: BTreeMap<String, Node>,
    /// The times `set_times` gave a path, as `FileTimes` holds them.
    times: BTreeMap<String, FileTimes>,
    cwd: String,
}

/// The shared state of a [`MemFs`] and its [`FixedOs`].
#[derive(Clone)]
pub struct MemHost(Rc<RefCell<State>>);

fn err(kind: HostErrorKind, what: &str) -> HostError {
    HostError::new(kind, what.to_string())
}

impl MemHost {
    pub fn new() -> MemHost {
        let mut nodes = BTreeMap::new();
        for d in ["/", "/work", "/tmp", "/home", "/home/puy"] {
            nodes.insert(d.to_string(), Node::Dir);
        }
        MemHost(Rc::new(RefCell::new(State {
            nodes,
            times: BTreeMap::new(),
            cwd: "/work".into(),
        })))
    }

    /// The file system.
    pub fn fs(&self) -> Box<dyn Fs> {
        Box::new(MemFs(self.clone()))
    }

    /// The OS, reporting `platform` (`linux`, `windows`, …) as its kind.
    pub fn os(&self, platform: &str) -> Box<dyn Os> {
        Box::new(FixedOs {
            host: self.clone(),
            platform: platform.to_string(),
        })
    }

    /// `path` made absolute and normalized.
    fn resolve(&self, path: &str) -> Result<String, HostError> {
        if path.is_empty() {
            return Err(err(HostErrorKind::NotFound, "empty path"));
        }
        let joined = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("{}/{path}", self.0.borrow().cwd)
        };
        let mut parts: Vec<&str> = Vec::new();
        for p in joined.split('/') {
            match p {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                p => parts.push(p),
            }
        }
        Ok(format!("/{}", parts.join("/")))
    }

    fn parent(path: &str) -> &str {
        match path.rfind('/') {
            Some(0) => "/",
            Some(i) => &path[..i],
            None => "/",
        }
    }

    fn kind(&self, path: &str) -> Option<FileType> {
        self.0.borrow().nodes.get(path).map(|n| match n {
            Node::Dir => FileType::Dir,
            Node::File(..) => FileType::File,
        })
    }

    /// Checks that the parent of `path` is a directory.
    fn check_parent(&self, path: &str) -> Result<(), HostError> {
        match self.kind(Self::parent(path)) {
            Some(FileType::Dir) => Ok(()),
            Some(_) => Err(err(HostErrorKind::Other, "not a directory")),
            None => Err(err(HostErrorKind::NotFound, "no such file or directory")),
        }
    }

    fn children(&self, dir: &str) -> Vec<String> {
        let prefix = if dir == "/" {
            "/".to_string()
        } else {
            format!("{dir}/")
        };
        self.0
            .borrow()
            .nodes
            .keys()
            .filter(|k| k.starts_with(&prefix) && k.len() > prefix.len())
            .filter(|k| !k[prefix.len()..].contains('/'))
            .cloned()
            .collect()
    }

    fn metadata(&self, path: &str) -> Result<Metadata, HostError> {
        let path = self.resolve(path)?;
        let state = self.0.borrow();
        let (file_type, len, readonly) = match state.nodes.get(&path) {
            None => return Err(err(HostErrorKind::NotFound, "no such file or directory")),
            Some(Node::Dir) => (FileType::Dir, 0, false),
            Some(Node::File(b, ro)) => (FileType::File, b.borrow().len() as u64, *ro),
        };
        let times = state.times.get(&path).copied().unwrap_or_default();
        Ok(Metadata {
            file_type,
            len,
            modified: Some(times.modified.unwrap_or(MEM_TIME)),
            accessed: Some(times.accessed.unwrap_or(MEM_TIME)),
            created: Some(times.created.unwrap_or(MEM_TIME)),
            readonly,
        })
    }

    fn create_dir(&self, path: &str, recursive: bool) -> Result<(), HostError> {
        let path = self.resolve(path)?;
        match self.kind(&path) {
            Some(FileType::Dir) if recursive => return Ok(()),
            Some(_) => return Err(err(HostErrorKind::AlreadyExists, "file exists")),
            None => {}
        }
        if recursive {
            let parent = Self::parent(&path).to_string();
            if self.kind(&parent).is_none() {
                self.create_dir(&parent, true)?;
            }
        }
        self.check_parent(&path)?;
        self.0.borrow_mut().nodes.insert(path, Node::Dir);
        Ok(())
    }

    fn remove(&self, path: &str, dir: bool) -> Result<(), HostError> {
        let path = self.resolve(path)?;
        match self.kind(&path) {
            None => return Err(err(HostErrorKind::NotFound, "no such file or directory")),
            Some(FileType::Dir) if !dir => {
                return Err(err(HostErrorKind::IsDirectory, "is a directory"));
            }
            Some(FileType::Dir) if !self.children(&path).is_empty() => {
                return Err(err(HostErrorKind::Other, "directory not empty"));
            }
            Some(FileType::Dir) if path == "/" => {
                return Err(err(HostErrorKind::Other, "resource busy"));
            }
            Some(FileType::Dir) => {}
            Some(_) if dir => return Err(err(HostErrorKind::Other, "not a directory")),
            Some(_) => {}
        }
        let mut state = self.0.borrow_mut();
        state.nodes.remove(&path);
        state.times.remove(&path);
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), HostError> {
        let from = self.resolve(from)?;
        let to = self.resolve(to)?;
        let Some(kind) = self.kind(&from) else {
            return Err(err(HostErrorKind::NotFound, "no such file or directory"));
        };
        self.check_parent(&to)?;
        if from == to {
            return Ok(());
        }
        match (kind, self.kind(&to)) {
            (_, None) => {}
            (FileType::Dir, Some(FileType::Dir)) if self.children(&to).is_empty() => {}
            (FileType::Dir, Some(_)) => {
                return Err(err(HostErrorKind::Other, "directory not empty"));
            }
            (_, Some(FileType::Dir)) => {
                return Err(err(HostErrorKind::IsDirectory, "is a directory"));
            }
            (_, Some(_)) => {}
        }
        if to.starts_with(&format!("{from}/")) {
            return Err(err(HostErrorKind::Other, "invalid argument"));
        }
        let mut state = self.0.borrow_mut();
        let moved: Vec<String> = state
            .nodes
            .keys()
            .filter(|k| **k == from || k.starts_with(&format!("{from}/")))
            .cloned()
            .collect();
        state.nodes.remove(&to);
        state.times.remove(&to);
        for k in moved {
            let node = state.nodes.remove(&k).expect("listed above");
            let moved_to = format!("{to}{}", &k[from.len()..]);
            if let Some(t) = state.times.remove(&k) {
                state.times.insert(moved_to.clone(), t);
            }
            state.nodes.insert(moved_to, node);
        }
        Ok(())
    }

    fn open(&self, path: &str, options: OpenOptions) -> Result<Box<dyn File>, HostError> {
        let path = self.resolve(path)?;
        let writes = options.write || options.append;
        let existing = match self.0.borrow().nodes.get(&path) {
            Some(Node::Dir) => return Err(err(HostErrorKind::IsDirectory, "is a directory")),
            Some(Node::File(b, ro)) => Some((Rc::clone(b), *ro)),
            None => None,
        };
        let bytes = match existing {
            Some(_) if options.create_new => {
                return Err(err(HostErrorKind::AlreadyExists, "file exists"));
            }
            Some((_, true)) if writes => {
                return Err(err(HostErrorKind::PermissionDenied, "permission denied"));
            }
            Some((b, _)) => b,
            None if writes && (options.create || options.create_new) => {
                self.check_parent(&path)?;
                let b = Rc::new(RefCell::new(Vec::new()));
                self.0
                    .borrow_mut()
                    .nodes
                    .insert(path, Node::File(Rc::clone(&b), false));
                b
            }
            None => return Err(err(HostErrorKind::NotFound, "no such file or directory")),
        };
        if options.truncate && writes {
            bytes.borrow_mut().clear();
        }
        Ok(Box::new(MemFile {
            bytes,
            pos: 0,
            read: options.read,
            write: writes,
            append: options.append,
        }))
    }
}

impl Default for MemHost {
    fn default() -> Self {
        Self::new()
    }
}

struct MemFs(MemHost);

impl Fs for MemFs {
    fn open(&mut self, path: &str, options: OpenOptions) -> HostFuture<Box<dyn File>> {
        ready(self.0.open(path, options))
    }

    fn metadata(&mut self, path: &str, _follow_links: bool) -> HostFuture<Metadata> {
        ready(self.0.metadata(path))
    }

    fn read_dir(&mut self, path: &str) -> HostFuture<Box<dyn DirReader>> {
        let r = self.0.resolve(path).and_then(|p| match self.0.kind(&p) {
            Some(FileType::Dir) => {
                let state = self.0.0.borrow();
                let entries: Vec<DirEntry> = self
                    .0
                    .children(&p)
                    .into_iter()
                    .map(|c| DirEntry {
                        name: c[c.rfind('/').expect("an absolute path") + 1..].to_string(),
                        file_type: match state.nodes.get(&c) {
                            Some(Node::Dir) => FileType::Dir,
                            _ => FileType::File,
                        },
                    })
                    .collect();
                Ok(Box::new(MemDirReader(entries)) as Box<dyn DirReader>)
            }
            Some(_) => Err(err(HostErrorKind::Other, "not a directory")),
            None => Err(err(HostErrorKind::NotFound, "no such file or directory")),
        });
        ready(r)
    }

    fn create_dir(&mut self, path: &str, recursive: bool) -> HostFuture<()> {
        ready(self.0.create_dir(path, recursive))
    }

    fn remove_dir(&mut self, path: &str) -> HostFuture<()> {
        ready(self.0.remove(path, true))
    }

    fn remove_file(&mut self, path: &str) -> HostFuture<()> {
        ready(self.0.remove(path, false))
    }

    fn rename(&mut self, from: &str, to: &str) -> HostFuture<()> {
        ready(self.0.rename(from, to))
    }

    fn copy(&mut self, from: &str, to: &str) -> HostFuture<u64> {
        let r = self.0.open(from, OpenOptions::read()).and_then(|mut src| {
            let mut dst = self.0.open(to, OpenOptions::create())?;
            let bytes = poll_now(src.read(usize::MAX))?;
            poll_now(dst.write(&bytes))?;
            Ok(bytes.len() as u64)
        });
        ready(r)
    }

    fn set_times(&mut self, path: &str, times: FileTimes) -> HostFuture<()> {
        let r = self.0.metadata(path).and_then(|_| {
            let p = self.0.resolve(path)?;
            let mut state = self.0.0.borrow_mut();
            let t = state.times.entry(p).or_default();
            t.accessed = times.accessed.or(t.accessed);
            t.modified = times.modified.or(t.modified);
            t.created = times.created.or(t.created);
            Ok(())
        });
        ready(r)
    }

    fn set_readonly(&mut self, path: &str, readonly: bool) -> HostFuture<()> {
        let r = self.0.resolve(path).and_then(|p| {
            match self.0.0.borrow_mut().nodes.get_mut(&p) {
                Some(Node::File(_, ro)) => *ro = readonly,
                Some(Node::Dir) => {}
                None => return Err(err(HostErrorKind::NotFound, "no such file or directory")),
            }
            Ok(())
        });
        ready(r)
    }

    fn canonicalize(&mut self, path: &str) -> HostFuture<String> {
        ready(self.0.metadata(path).and_then(|_| self.0.resolve(path)))
    }

    fn read_link(&mut self, _path: &str) -> HostFuture<String> {
        ready(Err(err(HostErrorKind::Other, "not a symbolic link")))
    }
}

/// The result of a `HostFuture` of the memory host, which is always complete.
fn poll_now<T>(mut f: HostFuture<T>) -> Result<T, HostError> {
    use std::task::{Context, Poll, Waker};
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(r) => r,
        Poll::Pending => unreachable!("the memory host completes at once"),
    }
}

struct MemFile {
    bytes: Rc<RefCell<Vec<u8>>>,
    pos: u64,
    read: bool,
    write: bool,
    append: bool,
}

impl File for MemFile {
    fn read(&mut self, max: usize) -> HostFuture<Vec<u8>> {
        if !self.read {
            return ready(Err(err(HostErrorKind::Other, "bad file descriptor")));
        }
        let b = self.bytes.borrow();
        let start = (self.pos as usize).min(b.len());
        let end = start.saturating_add(max).min(b.len());
        self.pos = end as u64;
        ready(Ok(b[start..end].to_vec()))
    }

    fn write(&mut self, bytes: &[u8]) -> HostFuture<()> {
        if !self.write {
            return ready(Err(err(HostErrorKind::Other, "bad file descriptor")));
        }
        let mut b = self.bytes.borrow_mut();
        if self.append {
            self.pos = b.len() as u64;
        }
        let start = self.pos as usize;
        if b.len() < start {
            b.resize(start, 0);
        }
        let end = start + bytes.len();
        if b.len() < end {
            b.resize(end, 0);
        }
        b[start..end].copy_from_slice(bytes);
        self.pos = end as u64;
        ready(Ok(()))
    }

    fn seek(&mut self, pos: SeekFrom) -> HostFuture<u64> {
        let len = self.bytes.borrow().len() as i128;
        let new = match pos {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(d) => len + i128::from(d),
            SeekFrom::Current(d) => i128::from(self.pos) + i128::from(d),
        };
        if new < 0 {
            return ready(Err(err(HostErrorKind::Other, "invalid seek")));
        }
        self.pos = new as u64;
        ready(Ok(self.pos))
    }

    fn set_len(&mut self, len: u64) -> HostFuture<()> {
        self.bytes.borrow_mut().resize(len as usize, 0);
        ready(Ok(()))
    }

    fn flush(&mut self) -> HostFuture<()> {
        ready(Ok(()))
    }

    fn sync(&mut self) -> HostFuture<()> {
        ready(Ok(()))
    }

    fn metadata(&mut self) -> HostFuture<Metadata> {
        ready(Ok(Metadata {
            file_type: FileType::File,
            len: self.bytes.borrow().len() as u64,
            modified: None,
            accessed: None,
            created: None,
            readonly: false,
        }))
    }
}

struct MemDirReader(Vec<DirEntry>);

impl DirReader for MemDirReader {
    fn next_batch(&mut self, max: usize) -> HostFuture<Vec<DirEntry>> {
        let n = max.min(self.0.len());
        ready(Ok(self.0.drain(..n).collect()))
    }
}

struct FixedOs {
    host: MemHost,
    platform: String,
}

impl Os for FixedOs {
    fn process_id(&mut self) -> Result<u32, HostError> {
        Ok(4242)
    }

    fn cpu_count(&mut self) -> Result<u32, HostError> {
        Ok(4)
    }

    fn temp_dir(&mut self) -> Result<String, HostError> {
        Ok("/tmp".into())
    }

    fn home_dir(&mut self) -> Result<String, HostError> {
        Ok("/home/puy".into())
    }

    /// Fails, as on Unix, once the working directory has been removed.
    fn current_dir(&mut self) -> Result<String, HostError> {
        let cwd = self.host.0.borrow().cwd.clone();
        match self.host.kind(&cwd) {
            Some(FileType::Dir) => Ok(cwd),
            _ => {
                Err(err(HostErrorKind::NotFound, "no such file or directory").with_os_code(Some(2)))
            }
        }
    }

    fn set_current_dir(&mut self, path: &str) -> Result<(), HostError> {
        let p = self.host.resolve(path)?;
        match self.host.kind(&p) {
            Some(FileType::Dir) => {
                self.host.0.borrow_mut().cwd = p;
                Ok(())
            }
            Some(_) => Err(err(HostErrorKind::Other, "not a directory")),
            None => Err(err(HostErrorKind::NotFound, "no such file or directory")),
        }
    }

    fn os_version(&mut self) -> Result<OsVersion, HostError> {
        Ok(OsVersion {
            platform: self.platform.clone(),
            arch: "x86_64".into(),
            name: "Puy Test OS".into(),
            major: 1,
            minor: 2,
            build: 3,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Relative paths join the working directory; `..` and `.` resolve by name.
    #[test]
    fn paths_resolve_against_the_working_directory() {
        let h = MemHost::new();
        assert_eq!(h.resolve("a/./b/../c").unwrap(), "/work/a/c");
        assert_eq!(h.resolve("/../x").unwrap(), "/x");
    }

    // A recursive `create_dir` makes the parents, and a directory with children is not removed.
    #[test]
    fn directories_nest() {
        let h = MemHost::new();
        h.create_dir("a/b/c", true).unwrap();
        assert_eq!(h.kind("/work/a/b"), Some(FileType::Dir));
        assert_eq!(
            h.remove("a", true).map_err(|e| e.message),
            Err("directory not empty".to_string())
        );
        h.rename("a", "z").unwrap();
        assert_eq!(h.kind("/work/z/b/c"), Some(FileType::Dir));
        assert_eq!(h.kind("/work/a"), None);
    }
}
