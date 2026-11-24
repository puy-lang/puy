//! The snapshots of `Puy.Testing.AssertSnapshot` (`docs/spec/test.md` 12): the runner's side,
//! which alone reads and writes their files. Each test's isolate gets a [`TestSnapshots`] as its
//! `HostSet::test`; they share the [`Snapshots`] of the run.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use puy_host::{HostError, HostErrorKind, TestRunner};

/// The folder of the snapshots, next to the test unit.
const DIR: &str = "snapshots";
const EXT: &str = ".snap";

/// What the tests of a run did with the snapshots.
pub(super) struct Snapshots {
    update: bool,
    /// The path from the working directory of each test unit's file, by the path the module
    /// gives it (from the package root, or as it was passed; `docs/spec/test.md` 2).
    disk: HashMap<String, String>,
    /// The snapshot files the tests compared with or wrote (paths from the working directory).
    used: HashSet<PathBuf>,
    /// The folders (from the working directory) and file names without `.pas` of the units
    /// whose tests ran: where unused snapshots are counted.
    units: BTreeSet<(PathBuf, String)>,
    written: usize,
}

/// The counts reported after an `--update` run.
pub(super) struct SnapshotCounts {
    pub written: usize,
    pub unused: usize,
}

impl Snapshots {
    /// `files`: the test files compiled, as (path from the working directory, path from the
    /// package root).
    pub fn new(update: bool, files: &[(String, Option<String>)]) -> Rc<RefCell<Snapshots>> {
        let disk = files
            .iter()
            .map(|(path, rel)| (rel.clone().unwrap_or_else(|| path.clone()), path.clone()))
            .collect();
        Rc::new(RefCell::new(Snapshots {
            update,
            disk,
            used: HashSet::new(),
            units: BTreeSet::new(),
            written: 0,
        }))
    }

    /// The counts of an `--update` run: the snapshots written, and the snapshot files of the
    /// units whose tests ran that no test used (kept; `docs/spec/test.md` 12). `None` without
    /// `--update`.
    pub fn counts(&self) -> Option<SnapshotCounts> {
        if !self.update {
            return None;
        }
        // Every test unit compiled, run or not: a file of `A_test.B_test` is not one of `A_test`'s
        let all: BTreeSet<(PathBuf, &str)> = self
            .disk
            .values()
            .map(|p| (snapshot_dir(p), stem_of(file_name_of(p))))
            .collect();
        let mut unused = 0;
        for (dir, stem) in &self.units {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            let prefix = format!("{stem}.");
            // The longer names of other units that begin with this one's
            let longer: Vec<String> = all
                .iter()
                .filter(|(d, s)| d == dir && s.len() > stem.len() && s.starts_with(&prefix))
                .map(|(_, s)| format!("{s}."))
                .collect();
            for e in entries.flatten() {
                let name = e.file_name();
                let Some(name) = name.to_str() else { continue };
                if name.starts_with(&prefix)
                    && name.ends_with(EXT)
                    && !longer.iter().any(|l| name.starts_with(l.as_str()))
                    && !self.used.contains(&dir.join(name))
                {
                    unused += 1;
                }
            }
        }
        Some(SnapshotCounts {
            written: self.written,
            unused,
        })
    }
}

/// The snapshots of one test.
pub(super) struct TestSnapshots {
    shared: Rc<RefCell<Snapshots>>,
    /// The folder of the snapshots, from the working directory and as reported.
    dir: PathBuf,
    shown_dir: String,
    /// The first part of the file names: the unit's file name without `.pas` and the test's
    /// part (`Calc_test.Adds`, `Calc_test.Cases#2`).
    base: String,
    /// The number of unnamed snapshots so far.
    unnamed: u32,
    /// The names of the files the test used so far: one file holds one snapshot.
    seen: HashSet<String>,
}

impl TestSnapshots {
    /// The snapshots of the test `name` (`case`: its position among the `[TestCase]`s of its
    /// routine) of the unit `unit` whose file the module names `unit_path`.
    pub fn new(
        shared: &Rc<RefCell<Snapshots>>,
        unit: &str,
        unit_path: &str,
        name: &str,
        case: Option<u32>,
    ) -> TestSnapshots {
        let file = file_name_of(unit_path);
        let stem = stem_of(file);
        let shown_dir = format!("{}{DIR}", &unit_path[..unit_path.len() - file.len()]);
        let disk_unit = shared
            .borrow()
            .disk
            .get(unit_path)
            .cloned()
            .unwrap_or_else(|| unit_path.to_string());
        let dir = snapshot_dir(&disk_unit);
        shared
            .borrow_mut()
            .units
            .insert((dir.clone(), stem.to_string()));
        TestSnapshots {
            shared: Rc::clone(shared),
            dir,
            shown_dir,
            base: format!("{stem}.{}", test_part(unit, name, case)),
            unnamed: 0,
            seen: HashSet::new(),
        }
    }
}

/// The last part of a path (`/` or `\\` between the parts).
fn file_name_of(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A unit's file name without `.pas`.
fn stem_of(file: &str) -> &str {
    match file.len().checked_sub(4) {
        Some(n) if file.is_char_boundary(n) && file[n..].eq_ignore_ascii_case(".pas") => &file[..n],
        _ => file,
    }
}

/// The folder of the snapshots of the unit whose file is at `unit` (from the working
/// directory).
fn snapshot_dir(unit: &str) -> PathBuf {
    Path::new(unit).parent().unwrap_or(Path::new("")).join(DIR)
}

/// The test's part of a snapshot's file name: its name without the unit, with the position of
/// its `[TestCase]` (`#1`, `#2`, …) in place of its arguments (`docs/spec/test.md` 12).
fn test_part(unit: &str, name: &str, case: Option<u32>) -> String {
    let rest = name
        .strip_prefix(unit)
        .and_then(|r| r.strip_prefix('.'))
        .unwrap_or(name);
    match case {
        Some(c) => {
            let routine = rest.split_once('(').map_or(rest, |(r, _)| r);
            format!("{routine}#{}", c + 1)
        }
        // A second test of the same name (`A #2`; `docs/spec/attributes.md` 5.2)
        None => rest.replace(" #", "#"),
    }
}

impl TestRunner for TestSnapshots {
    fn snapshot(
        &mut self,
        name: Option<&str>,
        actual: &str,
        file: &str,
        line: u32,
    ) -> Result<Option<String>, HostError> {
        let suffix = match name {
            Some(n) => format!(".{n}"),
            None => {
                self.unnamed += 1;
                if self.unnamed == 1 {
                    String::new()
                } else {
                    format!(".{}", self.unnamed)
                }
            }
        };
        let file_name = format!("{}{suffix}{EXT}", self.base);
        let path = self.dir.join(&file_name);
        let shown = format!("{}/{file_name}", self.shown_dir);
        if !self.seen.insert(file_name.clone()) {
            // Under `--update` the second would overwrite the first, which then always fails
            return Ok(Some(format!(
                "snapshot `{}{suffix}` is used twice in the test (one file `{shown}` holds one snapshot)",
                self.base
            )));
        }
        let actual = to_lf(actual);
        let mut shared = self.shared.borrow_mut();
        shared.used.insert(path.clone());
        let stored = match std::fs::read(&path) {
            Ok(bytes) => Some(stored_value(&bytes, &shown)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(io_error(&e, "read", &shown)),
        };
        if stored.as_deref() == Some(actual.as_str()) {
            return Ok(None);
        }
        if shared.update {
            std::fs::create_dir_all(&self.dir).map_err(|e| io_error(&e, "write", &shown))?;
            let text = format!("// {file}:{line}\n{actual}");
            std::fs::write(&path, text).map_err(|e| io_error(&e, "write", &shown))?;
            shared.written += 1;
            return Ok(None);
        }
        let what = format!("snapshot `{}{suffix}`", self.base);
        Ok(Some(match stored {
            None => {
                format!("{what} does not exist: no file `{shown}` (`puy test --update` writes it)")
            }
            Some(stored) => {
                let mut m = format!(
                    "{what} differs from `{shown}` (`puy test --update` writes the new value)"
                );
                for l in unified_diff(&stored, &actual, &shown, "actual") {
                    m.push_str("\n    ");
                    m.push_str(&l);
                }
                m
            }
        }))
    }
}

/// The value stored in a snapshot file: what follows its first line (the position of the
/// call), with its line endings made `LF`.
fn stored_value(bytes: &[u8], shown: &str) -> Result<String, HostError> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        HostError::new(
            HostErrorKind::Other,
            format!("the snapshot file `{shown}` is not UTF-8"),
        )
    })?;
    let text = to_lf(text);
    Ok(match text.split_once('\n') {
        Some((_, value)) => value.to_string(),
        None => String::new(),
    })
}

fn to_lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn io_error(e: &std::io::Error, verb: &str, shown: &str) -> HostError {
    let mut h = HostError::from_io(e);
    h.message = format!("cannot {verb} the snapshot file `{shown}`: {}", h.message);
    h
}

/// A line of a text for the diff: its text and whether a line break ends it.
type Line<'a> = (&'a str, bool);

fn split_lines(s: &str) -> Vec<Line<'_>> {
    let mut out: Vec<Line<'_>> = s
        .split_inclusive('\n')
        .map(|l| match l.strip_suffix('\n') {
            Some(t) => (t, true),
            None => (l, false),
        })
        .collect();
    out.shrink_to_fit();
    out
}

/// The most cells of the table of the longest common subsequence: past it, the diff shows every
/// line as removed and added.
const MAX_DIFF_CELLS: usize = 16 << 20;
/// The lines kept around each change.
const CONTEXT: usize = 3;

/// The lines of a unified diff from `old` to `new` (without line breaks), with the names of the
/// two sides in the header.
fn unified_diff(old: &str, new: &str, old_name: &str, new_name: &str) -> Vec<String> {
    let a = split_lines(old);
    let b = split_lines(new);
    let ops = edit_script(&a, &b);
    let mut out = vec![format!("--- {old_name}"), format!("+++ {new_name}")];
    // Hunks: runs of changes with up to CONTEXT equal lines around them, two runs merged when
    // fewer than 2 * CONTEXT equal lines part them
    let changed: Vec<usize> = (0..ops.len()).filter(|&i| ops[i].0 != ' ').collect();
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(CONTEXT);
        let mut end = changed[k];
        while k + 1 < changed.len() && changed[k + 1] - end <= 2 * CONTEXT {
            k += 1;
            end = changed[k];
        }
        let end = (end + CONTEXT + 1).min(ops.len());
        k += 1;
        let (a_start, b_start) = ops[..start].iter().fold((0, 0), |(x, y), op| match op.0 {
            ' ' => (x + 1, y + 1),
            '-' => (x + 1, y),
            _ => (x, y + 1),
        });
        let hunk = &ops[start..end];
        let a_len = hunk.iter().filter(|op| op.0 != '+').count();
        let b_len = hunk.iter().filter(|op| op.0 != '-').count();
        out.push(format!(
            "@@ -{} +{} @@",
            range(a_start, a_len),
            range(b_start, b_len)
        ));
        for &(mark, (text, newline)) in hunk {
            out.push(format!("{mark}{text}"));
            if !newline {
                out.push("\\ No newline at end of file".to_string());
            }
        }
    }
    out
}

/// A range of a hunk's header: the first line (from 1) and the count, the count left out when
/// it is 1, the line before when it is 0.
fn range(start: usize, len: usize) -> String {
    match len {
        0 => format!("{start},0"),
        1 => format!("{}", start + 1),
        _ => format!("{},{len}", start + 1),
    }
}

/// The lines of `a` and `b` in order, marked ` ` (in both), `-` (only in `a`) or `+` (only in
/// `b`), by their longest common subsequence.
fn edit_script<'a>(a: &[Line<'a>], b: &[Line<'a>]) -> Vec<(char, Line<'a>)> {
    // The common lines at both ends need no table
    let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (ma, mb) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let mut out: Vec<(char, Line<'a>)> = a[..head].iter().map(|&l| (' ', l)).collect();
    let (n, m) = (ma.len(), mb.len());
    if n.saturating_mul(m) > MAX_DIFF_CELLS {
        out.extend(ma.iter().map(|&l| ('-', l)));
        out.extend(mb.iter().map(|&l| ('+', l)));
    } else {
        // lcs[i][j]: the length of the longest common subsequence of ma[i..] and mb[j..]
        let w = m + 1;
        let mut lcs = vec![0u32; (n + 1) * w];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i * w + j] = if ma[i] == mb[j] {
                    lcs[(i + 1) * w + j + 1] + 1
                } else {
                    lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && ma[i] == mb[j] {
                out.push((' ', ma[i]));
                i += 1;
                j += 1;
            } else if j == m || (i < n && lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]) {
                out.push(('-', ma[i]));
                i += 1;
            } else {
                out.push(('+', mb[j]));
                j += 1;
            }
        }
    }
    out.extend(a[a.len() - tail..].iter().map(|&l| (' ', l)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // A changed line among others shows as a hunk with up to three lines of context, and the
    // header counts the lines of each side.
    #[test]
    fn unified_diff_shows_a_hunk_with_context() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
        let new = "1\n2\n3\n4\nfive\n6\n7\n8\n9\n";
        assert_eq!(
            unified_diff(old, new, "a", "b"),
            [
                "--- a",
                "+++ b",
                "@@ -2,7 +2,7 @@",
                " 2",
                " 3",
                " 4",
                "-5",
                "+five",
                " 6",
                " 7",
                " 8"
            ]
        );
    }

    // Changes far apart make two hunks, each with its own context.
    #[test]
    fn unified_diff_splits_far_changes() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
        let new = "0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n";
        assert_eq!(
            unified_diff(old, new, "a", "b"),
            [
                "--- a",
                "+++ b",
                "@@ -1,3 +1,4 @@",
                "+0",
                " 1",
                " 2",
                " 3",
                "@@ -7,4 +8,3 @@",
                " 7",
                " 8",
                " 9",
                "-10"
            ]
        );
    }

    // A side with no line in the hunk is counted from the line before it (0 at the start).
    #[test]
    fn unified_diff_counts_an_empty_side_from_the_line_before() {
        assert_eq!(
            unified_diff("", "a\n", "x", "y"),
            ["--- x", "+++ y", "@@ -0,0 +1 @@", "+a"]
        );
        assert_eq!(
            unified_diff("a\nb\n", "a\n", "x", "y"),
            ["--- x", "+++ y", "@@ -1,2 +1 @@", " a", "-b"]
        );
    }

    // A last line without a line break is marked, so that only the line break may differ.
    #[test]
    fn unified_diff_marks_a_missing_final_line_break() {
        assert_eq!(
            unified_diff("a\nb", "a\nb\n", "x", "y"),
            [
                "--- x",
                "+++ y",
                "@@ -1,2 +1,2 @@",
                " a",
                "-b",
                "\\ No newline at end of file",
                "+b"
            ]
        );
    }

    // The test's part of the file name: the unit left out, the position of a `[TestCase]` in
    // place of its arguments, and a second test of the same name without its space.
    #[test]
    fn test_part_of_the_file_name() {
        for (name, case, part) in [
            ("U_test.Adds", None, "Adds"),
            ("U_test.TCalc.Adds", None, "TCalc.Adds"),
            ("U_test.Cases(1, 'a(b)')", Some(0), "Cases#1"),
            ("U_test.TCalc.Cases(2) #2", Some(3), "TCalc.Cases#4"),
            ("U_test.Again #2", None, "Again#2"),
        ] {
            assert_eq!(test_part("U_test", name, case), part, "{name}");
        }
    }
}
