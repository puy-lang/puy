//! Inline metadata (`(*! puy` ... `*)`; requirements §9.3, `docs/spec/lexical.md` 3.1,
//! `diagnostics.md` 4.6).
//!
//! M1 checks only the shape and keys. Resolving `[dependencies]` is M5.

use puy_core::diagnostics::{Code, Diagnostic, Severity, Stage};
use puy_core::source::{FileId, Span, TextRange};

const ALLOWED_KEYS: &[&str] = &["toolchain", "edition"];
const ALLOWED_SECTIONS: &[&str] = &["dependencies", "executable", "runtime"];

/// Checks the inline metadata at the start of the file (after the BOM and shebang, skipping
/// whitespace and newlines). Returns (diagnostics, additional file (`puy.toml`)).
pub fn check(path: &str, bytes: &[u8]) -> (Vec<Diagnostic>, Vec<(String, Vec<u8>)>) {
    let mut diags = Vec::new();
    let mut files = Vec::new();
    let Some((start, end)) = find_head_comment(bytes) else {
        return (diags, files);
    };
    let body = &bytes[start + 2..end - 2];
    if !puy_core::lexer::starts_like_inline_metadata(body) {
        return (diags, files);
    }
    let span = Span::new(
        FileId(0),
        TextRange::new((start as u32).into(), (end as u32).into()),
    );
    let rest = &body[b"! puy".len()..];
    let mut i = 0;
    while i < rest.len() && matches!(rest[i], b' ' | b'\t') {
        i += 1;
    }
    if !matches!(rest.get(i), Some(b'\n' | b'\r')) {
        diags.push(
            Diagnostic::new(
                Code::E(2507),
                Severity::Error,
                "invalid inline metadata: the first line must be `(*! puy`",
                Stage::Driver,
            )
            .with_primary(span, ""),
        );
        return (diags, files);
    }
    // Not allowed inside a project
    if let Some((manifest_path, manifest_bytes)) = find_manifest(path) {
        files.push((manifest_path.clone(), manifest_bytes));
        diags.push(
            Diagnostic::new(
                Code::E(2507),
                Severity::Error,
                "inline metadata is not allowed inside a project",
                Stage::Driver,
            )
            .with_primary(span, "")
            .with_secondary(
                Span::new(FileId(1), TextRange::empty(0.into())),
                "project manifest here",
            ),
        );
        return (diags, files);
    }
    // Contents (TOML shape and keys). Line numbers are file lines (the `(*! puy` line is
    // first_line)
    let first_line = 1 + bytes[..start]
        .iter()
        .enumerate()
        .filter(|&(k, &b)| b == b'\n' || (b == b'\r' && bytes.get(k + 1) != Some(&b'\n')))
        .count();
    let text = String::from_utf8_lossy(&rest[i..]);
    let mut section: Option<String> = None;
    for (n, line) in text.lines().enumerate() {
        let line_no = first_line + n;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(inner) = line.strip_prefix('[') {
            let Some(name) = inner.strip_suffix(']') else {
                diags.push(invalid(span, format!("cannot parse line {line_no}")));
                return (diags, files);
            };
            let name = name.trim();
            if !ALLOWED_SECTIONS.contains(&name) {
                diags.push(invalid(span, format!("unknown key `{name}`")));
                return (diags, files);
            }
            section = Some(name.to_string());
            continue;
        }
        let Some((key, _value)) = line.split_once('=') else {
            diags.push(invalid(span, format!("cannot parse line {line_no}")));
            return (diags, files);
        };
        let key = key.trim().trim_matches('"');
        if section.is_none() && !ALLOWED_KEYS.contains(&key) {
            diags.push(invalid(span, format!("unknown key `{key}`")));
            return (diags, files);
        }
    }
    (diags, files)
}

fn invalid(span: Span, what: String) -> Diagnostic {
    Diagnostic::new(
        Code::E(2507),
        Severity::Error,
        format!("invalid inline metadata: {what}"),
        Stage::Driver,
    )
    .with_primary(span, "")
}

/// Range of the leading block comment `(* ... *)` (located after skipping BOM, shebang, whitespace,
/// and newlines).
fn find_head_comment(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        i = 3;
    }
    if bytes[i..].starts_with(b"#!") {
        while i < bytes.len() && !matches!(bytes[i], b'\n' | b'\r') {
            i += 1;
        }
    }
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C) {
        i += 1;
    }
    if !bytes[i..].starts_with(b"(*") || bytes[i..].starts_with(b"(*$") {
        return None;
    }
    let start = i;
    let mut j = i + 2;
    while j + 1 < bytes.len() {
        if bytes[j] == b'*' && bytes[j + 1] == b')' {
            return Some((start, j + 2));
        }
        j += 1;
    }
    None
}

/// Searches upward from the file's directory for `puy.toml`. The returned path is for diagnostics:
/// it is joined with `/` starting from the directory of the path passed to `puy`, with `../`
/// repeated for levels above the working directory (an OS-independent form, same idea as `{$I}`
/// paths; `docs/spec/directives.md` chapter 5).
fn find_manifest(path: &str) -> Option<(String, Vec<u8>)> {
    let cwd = std::env::current_dir().ok()?;
    // Directory of the given path (relative form for display, and the actual path for reading)
    let given_dir: Option<std::path::PathBuf> = if path == "<stdin>" || path == "<eval>" {
        None
    } else {
        std::path::Path::new(path)
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .map(|d| d.to_path_buf())
    };
    let abs_start = match &given_dir {
        Some(d) if d.is_absolute() => d.clone(),
        Some(d) => cwd.join(d),
        None => cwd.clone(),
    };
    // For display: components of the given directory (if it contains `..`, use the OS path as is)
    let simple_rel: Option<Vec<String>> = match &given_dir {
        Some(d) if !d.is_absolute() => {
            let mut parts = Vec::new();
            for c in d.components() {
                match c {
                    std::path::Component::Normal(n) => parts.push(n.to_string_lossy().into_owned()),
                    std::path::Component::CurDir => {}
                    _ => return find_manifest_os(&abs_start),
                }
            }
            Some(parts)
        }
        Some(_) => return find_manifest_os(&abs_start),
        None => Some(Vec::new()),
    };
    let parts = simple_rel?;
    let mut dir = Some(abs_start.as_path());
    let mut level = 0usize;
    while let Some(d) = dir {
        let candidate = d.join("puy.toml");
        if let Ok(bytes) = std::fs::read(&candidate) {
            let shown = if level <= parts.len() {
                let mut s = parts[..parts.len() - level].join("/");
                if !s.is_empty() {
                    s.push('/');
                }
                s + "puy.toml"
            } else {
                "../".repeat(level - parts.len()) + "puy.toml"
            };
            return Some((shown, bytes));
        }
        dir = d.parent();
        level += 1;
    }
    None
}

/// Searches while displaying the OS path as is, e.g. when given an absolute path.
fn find_manifest_os(start: &std::path::Path) -> Option<(String, Vec<u8>)> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join("puy.toml");
        if let Ok(bytes) = std::fs::read(&candidate) {
            return Some((candidate.to_string_lossy().into_owned(), bytes));
        }
        dir = d.parent();
    }
    None
}

/// The package root (`docs/spec/units.md` 2.1): the nearest directory holding `puy.toml`, from the
/// directory of the passed file (the working directory for `-e` / stdin) upward. Returns how many
/// levels up it is and the file's path relative to it.
pub fn find_package_root(path: &str) -> Option<puy_core::driver::PackageRoot> {
    let cwd = std::env::current_dir().ok()?;
    let is_file = path != "<stdin>" && path != "<eval>";
    let start = if is_file {
        let p = std::path::Path::new(path);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            cwd.join(p)
        };
        abs.parent()?.to_path_buf()
    } else {
        cwd
    };
    // Fold `.` and `..` as text, without resolving links: the levels must match the parts of the
    // passed path that `root_prefix` drops (`docs/spec/units.md` 8.2)
    let start = {
        let mut out = std::path::PathBuf::new();
        for c in start.components() {
            match c {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    if !out.pop() {
                        out.push("..");
                    }
                }
                c => out.push(c.as_os_str()),
            }
        }
        out
    };
    let mut dir = Some(start.as_path());
    let mut levels = 0u32;
    while let Some(d) = dir {
        if d.join("puy.toml").is_file() {
            let file = if is_file {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|name| {
                        let rel = start.strip_prefix(d).ok()?;
                        let mut parts: Vec<String> = rel
                            .components()
                            .map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .collect();
                        parts.push(name.to_string());
                        Some(parts.join("/"))
                    })
            } else {
                None
            };
            return Some(puy_core::driver::PackageRoot { levels, file });
        }
        dir = d.parent();
        levels += 1;
    }
    None
}
