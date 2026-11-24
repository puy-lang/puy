//! The value of a subexpression a failed `Assert` records, as a line of text
//! (`docs/spec/semantics.md` 5.4): one rule for every type, read from the debug information
//! section (`docs/spec/bytecode.md` 3.2, section 14).

use super::heap::{BadHandle, Heap};
use crate::bytecode::debug::{self as dt, DebugSection, DebugType};

/// The number of Unicode scalar values a value is cut to (the rest becomes `…`).
const MAX_CHARS: usize = 100;
/// The number of elements of an array shown (the rest are counted).
const MAX_ELEMS: usize = 10;
/// The levels of records and arrays shown inside one another (deeper ones are `(…)` / `[…]`).
const MAX_DEPTH: usize = 3;

/// Where a value is: a slot holding it, or the slots of a container from `at` where it is laid
/// out flat (a container of 0 is a default value: every slot reads 0).
#[derive(Clone, Copy)]
enum Loc {
    Slot(i64),
    Flat { block: i64, at: usize },
}

/// The text of the value `v` of the debug type `ty` (1 and up).
pub(super) fn show(heap: &Heap, debug: Option<&DebugSection>, v: i64, ty: u32) -> String {
    let mut s = Shower {
        heap,
        debug,
        out: String::new(),
        chars: 0,
    };
    s.value(Loc::Slot(v), ty, 0);
    if s.chars > MAX_CHARS {
        let cut: String = s.out.chars().take(MAX_CHARS).collect();
        return cut + "…";
    }
    s.out
}

struct Shower<'a> {
    heap: &'a Heap,
    debug: Option<&'a DebugSection>,
    out: String,
    /// The scalar values written (writing stops a little past the limit).
    chars: usize,
}

impl Shower<'_> {
    fn full(&self) -> bool {
        self.chars > MAX_CHARS
    }

    fn push(&mut self, s: &str) {
        for c in s.chars() {
            if self.full() {
                return;
            }
            self.out.push(c);
            self.chars += 1;
        }
    }

    /// The slot `i` of a container (0 reads 0; a slot that cannot be read reads 0 too, as the
    /// value of a malformed program is not worth failing the report for).
    fn slot(&self, block: i64, i: usize) -> i64 {
        if block == 0 {
            return 0;
        }
        self.heap.any_slot(block, i).unwrap_or(0)
    }

    /// The value at `loc` read as one slot.
    fn read(&self, loc: Loc) -> i64 {
        match loc {
            Loc::Slot(v) => v,
            Loc::Flat { block, at } => self.slot(block, at),
        }
    }

    fn value(&mut self, loc: Loc, ty: u32, depth: usize) {
        if self.full() {
            return;
        }
        let Some(t) = self.debug.and_then(|d| d.entry(ty)) else {
            self.push("?");
            return;
        };
        match t {
            DebugType::Plain(kind) => {
                let v = self.read(loc);
                self.plain(*kind, v);
            }
            DebugType::Enum(values) => {
                let v = self.read(loc);
                match values.iter().find(|(o, _)| *o == v) {
                    Some((_, name)) => {
                        let name = name.clone();
                        self.push(&name);
                    }
                    None => self.push(&v.to_string()),
                }
            }
            DebugType::Set { base, big } => {
                let v = self.read(loc);
                let words: Vec<u64> = if *big {
                    if v == 0 {
                        Vec::new()
                    } else {
                        self.heap
                            .set_slots(v)
                            .map(|w| w.iter().map(|x| *x as u64).collect())
                            .unwrap_or_default()
                    }
                } else {
                    vec![v as u64]
                };
                self.push("[");
                let mut first = true;
                for (w, bits) in words.iter().enumerate() {
                    for b in 0..64 {
                        if bits & (1 << b) != 0 {
                            if !first {
                                self.push(", ");
                            }
                            first = false;
                            self.value(Loc::Slot((w * 64 + b) as i64), *base, depth + 1);
                        }
                    }
                }
                self.push("]");
            }
            DebugType::Record { fields, .. } => {
                if depth >= MAX_DEPTH {
                    self.push("(…)");
                    return;
                }
                let (block, at) = self.container(loc, t);
                self.push("(");
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push("; ");
                    }
                    self.push(&f.name);
                    self.push(": ");
                    let field = self.place(block, at + f.offset as usize, f.ty);
                    self.value(field, f.ty, depth + 1);
                }
                self.push(")");
            }
            DebugType::Array {
                elem, count, width, ..
            } => {
                if depth >= MAX_DEPTH {
                    self.push("[…]");
                    return;
                }
                let (block, at) = self.container(loc, t);
                self.elems(block, at, *count as usize, *elem, *width as usize, depth);
            }
            DebugType::DynArray { elem, width } => {
                if depth >= MAX_DEPTH {
                    self.push("[…]");
                    return;
                }
                let block = self.read(loc);
                let w = (*width as usize).max(1);
                let len = if block == 0 {
                    0
                } else {
                    self.heap.any_shape(block).map_or(0, |(_, n)| n / w)
                };
                self.elems(block, 0, len, *elem, w, depth);
            }
        }
    }

    /// The container and first slot of a record or static array at `loc`: its own block when the
    /// slot holds it, or the place it is laid out flat in.
    fn container(&self, loc: Loc, t: &DebugType) -> (i64, usize) {
        match loc {
            Loc::Flat { block, at } if t.flat().is_some() => (block, at),
            _ => (self.read(loc), 0),
        }
    }

    /// Where a field or element of type `ty` at slot `at` of `block` is.
    fn place(&self, block: i64, at: usize, ty: u32) -> Loc {
        if self
            .debug
            .and_then(|d| d.entry(ty))
            .is_some_and(|t| t.flat().is_some())
        {
            Loc::Flat { block, at }
        } else {
            Loc::Slot(self.slot(block, at))
        }
    }

    /// `[a, b, …]`: the first elements of an array, then the number of the others.
    fn elems(&mut self, block: i64, at: usize, len: usize, elem: u32, width: usize, depth: usize) {
        self.push("[");
        for i in 0..len.min(MAX_ELEMS) {
            if i > 0 {
                self.push(", ");
            }
            let e = self.place(block, at + i * width, elem);
            self.value(e, elem, depth + 1);
        }
        if len > MAX_ELEMS {
            self.push(&format!(", … {} more", len - MAX_ELEMS));
        }
        self.push("]");
    }

    fn plain(&mut self, kind: u8, v: i64) {
        match kind {
            dt::DT_INT => self.push(&v.to_string()),
            dt::DT_UINT => self.push(&(v as u64).to_string()),
            dt::DT_BOOL => self.push(if v != 0 { "True" } else { "False" }),
            dt::DT_CHAR => self.units(&[v as u16]),
            dt::DT_SINGLE => self.push(&real_text(f64::from(f32::from_bits(v as u32)), true)),
            dt::DT_DOUBLE => self.push(&real_text(f64::from_bits(v as u64), false)),
            dt::DT_CURRENCY => self.push(&currency_text(v)),
            dt::DT_STRING => {
                if v == 0 {
                    self.push("''");
                    return;
                }
                let units: Vec<u16> = match self.heap.get(v) {
                    Ok(s) => s.iter_units().take(MAX_CHARS * 2 + 2).collect(),
                    Err(_) => {
                        self.push("?");
                        return;
                    }
                };
                self.units(&units);
            }
            dt::DT_UTF8 => {
                if v == 0 {
                    self.push("''");
                    return;
                }
                let text: Vec<u16> = match self.heap.get_u8(v) {
                    Ok(b) => {
                        let end = b.len().min(MAX_CHARS * 4 + 4);
                        String::from_utf8_lossy(&b[..end]).encode_utf16().collect()
                    }
                    Err(_) => {
                        self.push("?");
                        return;
                    }
                };
                self.units(&text);
            }
            dt::DT_CLASS | dt::DT_INTF => {
                let text = self.instance(v);
                self.push(&text);
            }
            dt::DT_CLASSREF => {
                if v == 0 {
                    self.push("nil");
                } else {
                    let name = super::class_decl_name(self.heap.type_name((v - 1) as u32));
                    self.push(name);
                }
            }
            _ => self.push(if v == 0 { "nil" } else { "<procedure>" }),
        }
    }

    /// An instance: its class's declared name and its handle's number (`nil`, or a destroyed
    /// instance).
    fn instance(&self, h: i64) -> String {
        match self.heap.obj_class(h) {
            Ok(ty) => format!(
                "{}@{}",
                super::class_decl_name(self.heap.type_name(ty)),
                h & 0xFFFF_FFFF
            ),
            Err(BadHandle::Nil) => "nil".to_string(),
            Err(_) => "<destroyed>".to_string(),
        }
    }

    /// A string or `Char` as a Pascal literal: the printable characters in quotes (`'` doubled),
    /// a control character or lone surrogate as `#N` (`''` when empty).
    fn units(&mut self, units: &[u16]) {
        if units.is_empty() {
            self.push("''");
            return;
        }
        let mut quoted = false;
        for c in char::decode_utf16(units.iter().copied()) {
            let code = match c {
                Ok(ch) if (ch as u32) >= 0x20 && ch != '\u{7F}' => {
                    if !quoted {
                        self.push("'");
                        quoted = true;
                    }
                    if ch == '\'' {
                        self.push("''");
                    } else {
                        let mut b = [0u8; 4];
                        self.push(ch.encode_utf8(&mut b));
                    }
                    continue;
                }
                Ok(ch) => ch as u32,
                Err(e) => u32::from(e.unpaired_surrogate()),
            };
            if quoted {
                self.push("'");
                quoted = false;
            }
            self.push(&format!("#{code}"));
        }
        if quoted {
            self.push("'");
        }
    }
}

/// A real as the shortest decimal that reads back as the same value (`3.0`, `0.1`, `1e100`;
/// `NAN`, `INF`, `-INF`). `single` reads it back as a `Single`.
fn real_text(x: f64, single: bool) -> String {
    if x.is_nan() {
        return "NAN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "INF" } else { "-INF" }.into();
    }
    if single {
        format!("{:?}", x as f32)
    } else {
        format!("{x:?}")
    }
}

/// A `Currency` (10000 times the value) with four decimals.
fn currency_text(v: i64) -> String {
    let sign = if v < 0 { "-" } else { "" };
    let a = v.unsigned_abs();
    format!("{sign}{}.{:04}", a / 10000, a % 10000)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reals are shown as the shortest decimal that reads back the same (semantics.md 5.4).
    #[test]
    fn reals_are_shortest_round_trips() {
        let cases: [(f64, bool, &str); 7] = [
            (3.0, false, "3.0"),
            (0.1, false, "0.1"),
            (1e100, false, "1e100"),
            (-0.0, false, "-0.0"),
            (f64::NAN, false, "NAN"),
            (f64::NEG_INFINITY, false, "-INF"),
            (f64::from(0.1f32), true, "0.1"),
        ];
        for (x, single, want) in cases {
            assert_eq!(real_text(x, single), want, "{x}");
        }
    }

    // A currency keeps four decimals and its sign, also between -1 and 0 (semantics.md 5.4).
    #[test]
    fn currencies_have_four_decimals() {
        let cases = [
            (15000, "1.5000"),
            (-5000, "-0.5000"),
            (0, "0.0000"),
            (i64::MIN, "-922337203685477.5808"),
        ];
        for (v, want) in cases {
            assert_eq!(currency_text(v), want, "{v}");
        }
    }
}
