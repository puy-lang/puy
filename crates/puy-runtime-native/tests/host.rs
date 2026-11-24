//! The file system and the OS information of the native runtime (`docs/design/stdlib.md` 3.4.1),
//! through the `HostSet` it builds. File system tests work in a directory of their own under the
//! temporary directory, by absolute paths, so they can run in parallel.

// WASI has no temporary directory to work in, and no process ID
#![cfg(not(target_os = "wasi"))]

use std::task::{Context, Poll, Waker};

use puy_host::{
    Capability, FileTimes, FileType, HostError, HostErrorKind, HostFuture, HostSet, OpenOptions,
    SeekFrom,
};

/// The result of a `HostFuture`, which the native runtime completes at once.
fn done<T>(mut fut: HostFuture<T>) -> Result<T, HostError> {
    match fut.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(r) => r,
        Poll::Pending => panic!("the native runtime completes host futures at once"),
    }
}

fn ok<T>(fut: HostFuture<T>) -> T {
    done(fut).expect("host operation")
}

fn kind<T>(fut: HostFuture<T>) -> HostErrorKind {
    match done(fut) {
        Ok(_) => panic!("the operation should fail"),
        Err(e) => e.kind,
    }
}

/// A fresh directory for one test, removed with everything in it when dropped.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("puy-host-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the scratch directory");
        Scratch(dir)
    }

    /// The absolute path of `name` inside the directory.
    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Held by the tests that read or change the working directory, which is one per process.
static WORKING_DIR: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn host() -> HostSet {
    puy_runtime_native::host_set()
}

// Every capability of the native runtime is there.
#[test]
fn native_host_set_has_every_capability() {
    let host = host();
    for cap in Capability::ALL {
        assert_eq!(host.absent(cap), None, "{cap:?}");
    }
}

// A file written through `File` reads back the same, and the position, the length and the
// attributes follow the writes.
#[test]
fn file_write_seek_read() {
    let dir = Scratch::new("file_write_seek_read");
    let mut host = host();
    let fs = host.fs().unwrap();
    let path = dir.path("a.bin");
    let mut f = ok(fs.open(&path, OpenOptions::create()));
    ok(f.write(b"hello, world"));
    ok(f.flush());
    ok(f.sync());
    assert_eq!(ok(f.seek(SeekFrom::Current(0))), 12);
    assert_eq!(ok(f.seek(SeekFrom::End(-5))), 7);
    ok(f.write(b"WORLD"));
    ok(f.set_len(10));
    let m = ok(f.metadata());
    assert_eq!(
        (m.file_type, m.len, m.readonly),
        (FileType::File, 10, false)
    );
    assert!(m.modified.is_some());
    drop(f);

    let mut f = ok(fs.open(&path, OpenOptions::read()));
    assert_eq!(ok(f.read(4)), b"hell");
    assert_eq!(ok(f.read(100)), b"o, WOR");
    assert_eq!(ok(f.read(100)), b"");
    assert_eq!(ok(f.seek(SeekFrom::Start(7))), 7);
    assert_eq!(ok(f.read(100)), b"WOR");
}

// `append` writes at the end whatever the position, and `create_new` refuses an existing file.
#[test]
fn file_append_and_create_new() {
    let dir = Scratch::new("file_append_and_create_new");
    let mut host = host();
    let fs = host.fs().unwrap();
    let path = dir.path("log.txt");
    let new = OpenOptions {
        write: true,
        create_new: true,
        ..OpenOptions::default()
    };
    ok(ok(fs.open(&path, new)).write(b"one\n"));
    assert_eq!(kind(fs.open(&path, new)), HostErrorKind::AlreadyExists);
    let append = OpenOptions {
        append: true,
        ..OpenOptions::default()
    };
    let mut f = ok(fs.open(&path, append));
    ok(f.seek(SeekFrom::Start(0)));
    ok(f.write(b"two\n"));
    drop(f);
    assert_eq!(std::fs::read(&path).unwrap(), b"one\ntwo\n");
}

// Opening a missing file is `NotFound`, and opening a directory is `IsDirectory` on every OS.
#[test]
fn open_errors_by_kind() {
    let dir = Scratch::new("open_errors_by_kind");
    let mut host = host();
    let fs = host.fs().unwrap();
    assert_eq!(
        kind(fs.open(&dir.path("missing"), OpenOptions::read())),
        HostErrorKind::NotFound
    );
    // With and without a separator at the end, and opened for writing
    let sub = dir.path("sub");
    ok(fs.create_dir(&sub, false));
    for path in [sub.clone(), dir.path("")] {
        for options in [OpenOptions::read(), OpenOptions::create()] {
            assert_eq!(
                kind(fs.open(&path, options)),
                HostErrorKind::IsDirectory,
                "{path} {options:?}"
            );
        }
    }
    assert_eq!(
        kind(fs.metadata(&dir.path("missing"), true)),
        HostErrorKind::NotFound
    );
}

// A listing comes in batches of at most `max`, ends with an empty batch, and gives each entry's
// name and type.
#[test]
fn read_dir_in_batches() {
    let dir = Scratch::new("read_dir_in_batches");
    let mut host = host();
    let fs = host.fs().unwrap();
    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        std::fs::write(dir.path(name), name).unwrap();
    }
    ok(fs.create_dir(&dir.path("sub"), false));
    let mut r = ok(fs.read_dir(&dir.path("")));
    let mut sizes = Vec::new();
    let mut entries = Vec::new();
    loop {
        let batch = ok(r.next_batch(2));
        sizes.push(batch.len());
        if batch.is_empty() {
            break;
        }
        entries.extend(batch.into_iter().map(|e| (e.name, e.file_type)));
    }
    assert_eq!(sizes, [2, 2, 1, 0]);
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let f = FileType::File;
    assert_eq!(
        entries,
        [
            ("a.txt".to_string(), f),
            ("b.txt".to_string(), f),
            ("c.txt".to_string(), f),
            ("d.txt".to_string(), f),
            ("sub".to_string(), FileType::Dir),
        ]
    );
    assert_eq!(
        kind(fs.read_dir(&dir.path("missing"))),
        HostErrorKind::NotFound
    );
}

// Directories are made (with their parents when recursive) and removed; files are renamed,
// copied and removed.
#[test]
fn create_rename_copy_remove() {
    let dir = Scratch::new("create_rename_copy_remove");
    let mut host = host();
    let fs = host.fs().unwrap();
    let deep = dir.path("x/y/z");
    assert_eq!(kind(fs.create_dir(&deep, false)), HostErrorKind::NotFound);
    ok(fs.create_dir(&deep, true));
    ok(fs.create_dir(&deep, true));
    assert_eq!(
        kind(fs.create_dir(&deep, false)),
        HostErrorKind::AlreadyExists
    );
    assert_eq!(ok(fs.metadata(&deep, true)).file_type, FileType::Dir);

    let a = dir.path("a.txt");
    std::fs::write(&a, "abc").unwrap();
    let b = dir.path("x/b.txt");
    ok(fs.rename(&a, &b));
    assert_eq!(kind(fs.metadata(&a, true)), HostErrorKind::NotFound);
    assert_eq!(ok(fs.copy(&b, &a)), 3);
    assert_eq!(std::fs::read_to_string(&a).unwrap(), "abc");
    ok(fs.remove_file(&a));
    assert_eq!(kind(fs.remove_file(&a)), HostErrorKind::NotFound);

    ok(fs.remove_dir(&deep));
    assert_eq!(kind(fs.metadata(&deep, true)), HostErrorKind::NotFound);
    // Not empty
    assert!(done(fs.remove_dir(&dir.path("x"))).is_err());
}

// The times set are the times read back; the creation time is settable only on Windows.
#[test]
fn set_times_reads_back() {
    let dir = Scratch::new("set_times_reads_back");
    let mut host = host();
    let fs = host.fs().unwrap();
    let path = dir.path("t.txt");
    std::fs::write(&path, "t").unwrap();
    // 2001-09-09T01:46:40Z
    let when = 1_000_000_000_000_000_000;
    let times = FileTimes {
        accessed: Some(when),
        modified: Some(when + 1_000_000_000),
        created: None,
    };
    ok(fs.set_times(&path, times));
    let m = ok(fs.metadata(&path, true));
    assert_eq!(m.modified, Some(when + 1_000_000_000));
    assert_eq!(m.accessed, Some(when));
    // A directory too
    ok(fs.set_times(&dir.path(""), times));
    assert_eq!(
        ok(fs.metadata(&dir.path(""), true)).modified,
        Some(when + 1_000_000_000)
    );

    // Before the epoch: 1960-01-01T00:00:00Z
    let before = -315_619_200_000_000_000;
    let old = FileTimes {
        modified: Some(before),
        ..FileTimes::default()
    };
    ok(fs.set_times(&path, old));
    assert_eq!(ok(fs.metadata(&path, true)).modified, Some(before));

    let created = FileTimes {
        created: Some(when),
        ..FileTimes::default()
    };
    if cfg!(windows) {
        ok(fs.set_times(&path, created));
        assert_eq!(ok(fs.metadata(&path, true)).created, Some(when));
    } else {
        assert_eq!(
            kind(fs.set_times(&path, created)),
            HostErrorKind::Unsupported
        );
    }
}

// The read-only attribute is set and cleared, and a file that is read-only again takes writes.
#[test]
fn set_readonly_and_back() {
    let dir = Scratch::new("set_readonly_and_back");
    let mut host = host();
    let fs = host.fs().unwrap();
    let path = dir.path("r.txt");
    std::fs::write(&path, "r").unwrap();
    ok(fs.set_readonly(&path, true));
    assert!(ok(fs.metadata(&path, true)).readonly);
    ok(fs.set_readonly(&path, false));
    assert!(!ok(fs.metadata(&path, true)).readonly);
    ok(ok(fs.open(&path, OpenOptions::create())).write(b"w"));
}

// `canonicalize` resolves `..` to an absolute path, and a missing path is `NotFound`.
#[test]
fn canonicalize_resolves_dots() {
    let dir = Scratch::new("canonicalize_resolves_dots");
    let mut host = host();
    let fs = host.fs().unwrap();
    ok(fs.create_dir(&dir.path("sub"), false));
    std::fs::write(dir.path("f.txt"), "f").unwrap();
    let dotted = std::path::Path::new(&dir.path("sub"))
        .join("..")
        .join("f.txt");
    let canon = ok(fs.canonicalize(&dotted.to_string_lossy()));
    assert!(std::path::Path::new(&canon).is_absolute(), "{canon}");
    assert!(!canon.contains(".."), "{canon}");
    assert!(!canon.starts_with(r"\\?\"), "{canon}");
    assert!(canon.ends_with("f.txt"), "{canon}");
    assert_eq!(
        kind(fs.canonicalize(&dir.path("missing"))),
        HostErrorKind::NotFound
    );
}

// The times of a named pipe are refused instead of waiting for a writer to open it.
#[cfg(unix)]
#[test]
fn set_times_refuses_a_pipe() {
    let dir = Scratch::new("set_times_refuses_a_pipe");
    let mut host = host();
    let fs = host.fs().unwrap();
    let fifo = dir.path("fifo");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(made.success());
    let times = FileTimes {
        modified: Some(0),
        ..FileTimes::default()
    };
    assert_eq!(kind(fs.set_times(&fifo, times)), HostErrorKind::Unsupported);
}

// A symbolic link is read as a link without following, as its target with following, and
// `read_link` gives the target as stored.
#[cfg(unix)]
#[test]
fn symlink_metadata_and_target() {
    let dir = Scratch::new("symlink_metadata_and_target");
    let mut host = host();
    let fs = host.fs().unwrap();
    std::fs::write(dir.path("target.txt"), "12345").unwrap();
    std::os::unix::fs::symlink("target.txt", dir.path("link")).unwrap();
    let link = dir.path("link");
    assert_eq!(ok(fs.metadata(&link, false)).file_type, FileType::Symlink);
    let m = ok(fs.metadata(&link, true));
    assert_eq!((m.file_type, m.len), (FileType::File, 5));
    assert_eq!(ok(fs.read_link(&link)), "target.txt");
    let names: Vec<_> = ok(ok(fs.read_dir(&dir.path(""))).next_batch(10))
        .into_iter()
        .filter(|e| e.name == "link")
        .map(|e| e.file_type)
        .collect();
    assert_eq!(names, [FileType::Symlink]);
    // Removing the link leaves the target
    ok(fs.remove_file(&link));
    assert_eq!(ok(fs.metadata(&dir.path("target.txt"), true)).len, 5);
}

// The values of the process are the ones std gives.
#[test]
fn os_values_of_the_process() {
    let _cwd = WORKING_DIR.lock().unwrap_or_else(|e| e.into_inner());
    let mut host = host();
    let os = host.os().unwrap();
    assert_eq!(os.process_id().unwrap(), std::process::id());
    assert!(os.cpu_count().unwrap() >= 1);
    assert_eq!(
        os.temp_dir().unwrap(),
        std::env::temp_dir().to_string_lossy()
    );
    assert_eq!(
        os.current_dir().unwrap(),
        std::env::current_dir().unwrap().to_string_lossy()
    );
    let v = os.os_version().unwrap();
    assert_eq!(v.platform, std::env::consts::OS);
    assert_eq!(v.arch, std::env::consts::ARCH);
    assert!(!v.name.is_empty());
    if cfg!(any(target_os = "linux", target_os = "macos", windows)) {
        assert!(v.major > 0, "{v:?}");
    }
}

// Changing the working directory is seen by the next read, and a missing directory is
// `NotFound`. The only test that changes it; it puts it back.
#[test]
fn set_current_dir_round_trip() {
    let _cwd = WORKING_DIR.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("set_current_dir_round_trip");
    let mut host = host();
    let os = host.os().unwrap();
    let before = os.current_dir().unwrap();
    os.set_current_dir(&dir.path("")).unwrap();
    let now = os.current_dir().unwrap();
    os.set_current_dir(&before).unwrap();
    assert_eq!(
        std::fs::canonicalize(&now).unwrap(),
        std::fs::canonicalize(&dir.0).unwrap()
    );
    assert_eq!(
        os.set_current_dir(&dir.path("missing")).unwrap_err().kind,
        HostErrorKind::NotFound
    );
}

// The list of environment variables is the process's.
#[test]
fn env_vars_are_the_processes() {
    let mut host = host();
    let env = host.env.as_mut().unwrap();
    let mut got = env.vars().unwrap();
    let mut want: Vec<_> = std::env::vars_os()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
    if let Some((name, value)) = got.first() {
        assert_eq!(env.get(name).unwrap().as_ref(), Some(value));
    }
}
