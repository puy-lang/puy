//! Bytecode (`docs/spec/bytecode.md`): instruction format, reading, writing and verifying modules (`.puyc`), and disassembly.

pub mod op;

use std::fmt;

/// Format version (3.1).
pub const FORMAT_VERSION: u32 = 1;

/// Builds an instruction word.
pub fn encode(op: u8, a: u32, b: u32, c: u32) -> u32 {
    op as u32 | ((a & 0xFF) << 8) | ((b & 0xFF) << 16) | ((c & 0xFF) << 24)
}

/// Extension word (upper 8 bits of `A` / `B` / `C`).
pub fn encode_ext(a: u32, b: u32, c: u32) -> u32 {
    (((a >> 8) & 0xFF) << 8) | (((b >> 8) & 0xFF) << 16) | (((c >> 8) & 0xFF) << 24)
}

/// Splits a word: `(op, A, B, C)` (8 bits each).
pub fn decode(word: u32) -> (u8, u32, u32, u32) {
    (
        (word & 0xFF) as u8,
        (word >> 8) & 0xFF,
        (word >> 16) & 0xFF,
        (word >> 24) & 0xFF,
    )
}

/// `Bx` joins `B/C` of the main word as the lower 16 bits and `B/C` of the extension word as the upper 16 bits.
/// Do not overlap `B` and `C` widened to 16 bits by shifting one of them by 8 bits.
pub(crate) fn decode_bx(word: u32, extension: Option<u32>) -> u32 {
    (word >> 16) | extension.map_or(0, |ext| ext & 0xFFFF_0000)
}

/// A constant pool entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstEntry {
    Int(i64),
    /// Kind 2: a UTF-8 string.
    Str(String),
    /// Kind 4: a sequence of UTF-16 code units (may contain lone surrogates).
    Utf16(Vec<u16>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    /// Fully qualified name (`Hello.Square`).
    pub name: String,
    pub params: u16,
    pub regs: u16,
    pub code: Vec<u32>,
    /// Line table: (word position, line), in ascending order of position.
    pub lines: Vec<(u32, u32)>,
    /// Local variable names: (register, name).
    pub locals: Vec<(u16, String)>,
    /// Source file (index into the unit section).
    pub file: u32,
}

impl Function {
    /// The line of a word position.
    pub fn line_at(&self, pos: u32) -> u32 {
        let mut line = self.lines.first().map(|l| l.1).unwrap_or(1);
        for &(p, l) in &self.lines {
            if p <= pos {
                line = l;
            } else {
                break;
            }
        }
        line
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalEntry {
    pub name: String,
    /// 0 = integer (including `Boolean` / `Char`), 1 = string.
    pub kind: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitEntry {
    pub name: String,
    pub path: String,
    pub init: Option<u32>,
    pub fini: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    pub name: String,
    pub params: u16,
    pub ret: bool,
}

/// A verified module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub consts: Vec<ConstEntry>,
    pub funcs: Vec<Function>,
    pub globals: Vec<GlobalEntry>,
    pub entry: u32,
    pub units: Vec<UnitEntry>,
    pub hosts: Vec<HostEntry>,
}

/// A load failure (F0004).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// `invalid bytecode module: …`
    Invalid(String),
    /// `bytecode was built with puy …, this is puy …`
    Version { built: String, this: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Invalid(m) => write!(f, "invalid bytecode module: {m}"),
            LoadError::Version { built, this } => {
                write!(f, "bytecode was built with puy {built}, this is puy {this}")
            }
        }
    }
}

const NONE_INDEX: u32 = 0xFFFF_FFFF;

// ---- Writing ----

struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn pad4(&mut self) {
        while !self.buf.len().is_multiple_of(4) {
            self.buf.push(0);
        }
    }
    fn lp_utf8(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self.pad4();
    }
}

impl Module {
    /// Encodes as `.puyc` bytes.
    pub fn encode(&self, puy_version: &str) -> Vec<u8> {
        let sections: Vec<(u32, Vec<u8>)> = vec![
            (1, self.encode_consts()),
            (2, self.encode_funcs()),
            (3, self.encode_globals()),
            (4, self.encode_units()),
            (5, self.encode_hosts()),
        ];
        let mut w = Writer { buf: Vec::new() };
        w.buf.extend_from_slice(b"PUYC");
        w.u32(FORMAT_VERSION);
        w.lp_utf8(puy_version);
        w.u32(sections.len() as u32);
        let table_at = w.buf.len();
        for _ in &sections {
            w.u32(0);
            w.u32(0);
            w.u32(0);
        }
        let mut offsets = Vec::new();
        for (kind, body) in &sections {
            w.pad4();
            offsets.push((*kind, w.buf.len() as u32, body.len() as u32));
            w.buf.extend_from_slice(body);
        }
        w.pad4();
        for (i, (kind, off, len)) in offsets.iter().enumerate() {
            let at = table_at + i * 12;
            w.buf[at..at + 4].copy_from_slice(&kind.to_le_bytes());
            w.buf[at + 4..at + 8].copy_from_slice(&off.to_le_bytes());
            w.buf[at + 8..at + 12].copy_from_slice(&len.to_le_bytes());
        }
        w.buf
    }

    fn encode_consts(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.consts.len() as u32);
        for c in &self.consts {
            match c {
                ConstEntry::Int(v) => {
                    w.u8(1);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.i64(*v);
                }
                ConstEntry::Str(s) => {
                    w.u8(2);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.lp_utf8(s);
                }
                ConstEntry::Utf16(u) => {
                    w.u8(4);
                    w.u8(0);
                    w.u8(0);
                    w.u8(0);
                    w.u32(u.len() as u32);
                    for x in u {
                        w.u16(*x);
                    }
                    w.pad4();
                }
            }
        }
        w.buf
    }

    fn encode_funcs(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.funcs.len() as u32);
        for f in &self.funcs {
            w.lp_utf8(&f.name);
            w.u16(f.params);
            w.u16(f.regs);
            w.u32(f.code.len() as u32);
            for x in &f.code {
                w.u32(*x);
            }
            w.u32(0); // handler table (M2)
            w.u32(0); // managed-type registers (M2)
            w.u32(f.lines.len() as u32);
            for (p, l) in &f.lines {
                w.u32(*p);
                w.u32(*l);
            }
            w.u32(f.locals.len() as u32);
            for (r, n) in &f.locals {
                w.u16(*r);
                w.u16(0);
                w.lp_utf8(n);
            }
            w.u32(f.file);
        }
        w.buf
    }

    fn encode_globals(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.globals.len() as u32);
        for g in &self.globals {
            w.lp_utf8(&g.name);
            w.u32(g.kind);
        }
        w.buf
    }

    fn encode_units(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.entry);
        w.u32(self.units.len() as u32);
        for u in &self.units {
            w.lp_utf8(&u.name);
            w.lp_utf8(&u.path);
            w.u32(u.init.unwrap_or(NONE_INDEX));
            w.u32(u.fini.unwrap_or(NONE_INDEX));
        }
        w.buf
    }

    fn encode_hosts(&self) -> Vec<u8> {
        let mut w = Writer { buf: Vec::new() };
        w.u32(self.hosts.len() as u32);
        for h in &self.hosts {
            w.lp_utf8(&h.name);
            w.u16(h.params);
            w.u16(h.ret as u16);
        }
        w.buf
    }
}

// ---- Reading ----

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn need(&self, n: usize) -> Result<(), LoadError> {
        if self.pos + n > self.buf.len() {
            Err(LoadError::Invalid("unexpected end of section".into()))
        } else {
            Ok(())
        }
    }
    fn u8(&mut self) -> Result<u8, LoadError> {
        self.need(1)?;
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, LoadError> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, LoadError> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }
    fn i64(&mut self) -> Result<i64, LoadError> {
        self.need(8)?;
        let v = i64::from_le_bytes(self.buf[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        Ok(v)
    }
    fn pad4(&mut self) -> Result<(), LoadError> {
        while !self.pos.is_multiple_of(4) {
            self.u8()?;
        }
        Ok(())
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], LoadError> {
        self.need(n)?;
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn lp_utf8(&mut self) -> Result<String, LoadError> {
        let n = self.u32()? as usize;
        let b = self.bytes(n)?;
        let s = std::str::from_utf8(b)
            .map_err(|_| LoadError::Invalid("string is not valid UTF-8".into()))?
            .to_string();
        self.pad4()?;
        Ok(s)
    }
    fn count(&mut self, what: &str) -> Result<usize, LoadError> {
        let n = self.u32()? as usize;
        // A count larger than the remaining bytes is invalid
        if n > self.buf.len() - self.pos {
            return Err(LoadError::Invalid(format!("{what} count is too large")));
        }
        Ok(n)
    }
}

impl Module {
    /// Reads from bytes and verifies (3.3).
    pub fn decode(bytes: &[u8], puy_version: &str) -> Result<Module, LoadError> {
        let mut r = Reader { buf: bytes, pos: 0 };
        if r.bytes(4)? != b"PUYC" {
            return Err(LoadError::Invalid("bad magic".into()));
        }
        let fv = r.u32()?;
        if fv != FORMAT_VERSION {
            return Err(LoadError::Invalid(format!(
                "unsupported format version {fv}"
            )));
        }
        let built = r.lp_utf8()?;
        if built != puy_version {
            return Err(LoadError::Version {
                built,
                this: puy_version.to_string(),
            });
        }
        let k = r.u32()? as usize;
        if k > 64 {
            return Err(LoadError::Invalid("too many sections".into()));
        }
        let mut table: Vec<(u32, u32, u32)> = Vec::new();
        for _ in 0..k {
            table.push((r.u32()?, r.u32()?, r.u32()?));
        }
        let mut sections: [Option<&[u8]>; 6] = [None; 6];
        let mut last_end = 0u32;
        for (kind, off, len) in table {
            let end = off
                .checked_add(len)
                .ok_or_else(|| LoadError::Invalid("section out of range".into()))?;
            if end as usize > bytes.len() || off < last_end {
                return Err(LoadError::Invalid("section out of range".into()));
            }
            last_end = end;
            if !(1..=5).contains(&kind) {
                return Err(LoadError::Invalid(format!("unknown section {kind}")));
            }
            if sections[kind as usize].is_some() {
                return Err(LoadError::Invalid(format!("duplicate section {kind}")));
            }
            sections[kind as usize] = Some(&bytes[off as usize..end as usize]);
        }
        if let Some(kind) = (1..=5).find(|k| sections[*k].is_none()) {
            return Err(LoadError::Invalid(format!("missing section {kind}")));
        }
        let consts = Self::decode_consts(sections[1].unwrap())?;
        let funcs = Self::decode_funcs(sections[2].unwrap())?;
        let globals = Self::decode_globals(sections[3].unwrap())?;
        let (entry, units) = Self::decode_units(sections[4].unwrap())?;
        let hosts = Self::decode_hosts(sections[5].unwrap())?;
        let m = Module {
            consts,
            funcs,
            globals,
            entry,
            units,
            hosts,
        };
        m.verify()?;
        Ok(m)
    }

    fn decode_consts(buf: &[u8]) -> Result<Vec<ConstEntry>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("constant")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let kind = r.u8()?;
            r.u8()?;
            r.u8()?;
            r.u8()?;
            out.push(match kind {
                1 => ConstEntry::Int(r.i64()?),
                2 => ConstEntry::Str(r.lp_utf8()?),
                4 => {
                    let c = r.u32()? as usize;
                    if c > buf.len() {
                        return Err(LoadError::Invalid("string constant is too long".into()));
                    }
                    let mut u = Vec::with_capacity(c);
                    for _ in 0..c {
                        u.push(r.u16()?);
                    }
                    r.pad4()?;
                    if !u.iter().any(|x| (0xD800..0xE000).contains(x))
                        || String::from_utf16(&u).is_ok()
                    {
                        return Err(LoadError::Invalid(
                            "UTF-16 constant without a lone surrogate".into(),
                        ));
                    }
                    ConstEntry::Utf16(u)
                }
                k => return Err(LoadError::Invalid(format!("unknown constant kind {k}"))),
            });
        }
        Ok(out)
    }

    fn decode_funcs(buf: &[u8]) -> Result<Vec<Function>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("function")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.lp_utf8()?;
            let params = r.u16()?;
            let regs = r.u16()?;
            let nwords = r.count("code")?;
            let mut code = Vec::with_capacity(nwords);
            for _ in 0..nwords {
                code.push(r.u32()?);
            }
            let handlers = r.u32()?;
            if handlers != 0 {
                return Err(LoadError::Invalid("handler table is not supported".into()));
            }
            let managed = r.u32()?;
            if managed != 0 {
                return Err(LoadError::Invalid(
                    "managed register list is not supported".into(),
                ));
            }
            let nlines = r.count("line table")?;
            let mut lines = Vec::with_capacity(nlines);
            for _ in 0..nlines {
                lines.push((r.u32()?, r.u32()?));
            }
            let nlocals = r.count("local")?;
            let mut locals = Vec::with_capacity(nlocals);
            for _ in 0..nlocals {
                let reg = r.u16()?;
                r.u16()?;
                locals.push((reg, r.lp_utf8()?));
            }
            let file = r.u32()?;
            out.push(Function {
                name,
                params,
                regs,
                code,
                lines,
                locals,
                file,
            });
        }
        Ok(out)
    }

    fn decode_globals(buf: &[u8]) -> Result<Vec<GlobalEntry>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("global")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.lp_utf8()?;
            let kind = r.u32()?;
            if kind > 1 {
                return Err(LoadError::Invalid(format!("unknown global kind {kind}")));
            }
            out.push(GlobalEntry { name, kind });
        }
        Ok(out)
    }

    fn decode_units(buf: &[u8]) -> Result<(u32, Vec<UnitEntry>), LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let entry = r.u32()?;
        let n = r.count("unit")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.lp_utf8()?;
            let path = r.lp_utf8()?;
            let init = r.u32()?;
            let fini = r.u32()?;
            out.push(UnitEntry {
                name,
                path,
                init: (init != NONE_INDEX).then_some(init),
                fini: (fini != NONE_INDEX).then_some(fini),
            });
        }
        Ok((entry, out))
    }

    fn decode_hosts(buf: &[u8]) -> Result<Vec<HostEntry>, LoadError> {
        let mut r = Reader { buf, pos: 0 };
        let n = r.count("host function")?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.lp_utf8()?;
            let params = r.u16()?;
            let ret = r.u16()?;
            if ret > 1 {
                return Err(LoadError::Invalid("bad host function return flag".into()));
            }
            out.push(HostEntry {
                name,
                params,
                ret: ret == 1,
            });
        }
        Ok(out)
    }

    // ---- Verification (3.3) ----

    pub fn verify(&self) -> Result<(), LoadError> {
        if self.units.is_empty() {
            return Err(LoadError::Invalid("no unit".into()));
        }
        let check_fn = |idx: u32, what: &str| -> Result<(), LoadError> {
            let f = self
                .funcs
                .get(idx as usize)
                .ok_or_else(|| LoadError::Invalid(format!("{what} index out of range")))?;
            if f.params != 0 {
                return Err(LoadError::Invalid(format!(
                    "{what} function takes parameters"
                )));
            }
            Ok(())
        };
        check_fn(self.entry, "entry")?;
        for u in &self.units {
            if let Some(i) = u.init {
                check_fn(i, "initialization")?;
            }
            if let Some(i) = u.fini {
                check_fn(i, "finalization")?;
            }
        }
        for (fi, f) in self.funcs.iter().enumerate() {
            self.verify_function(fi, f)?;
        }
        Ok(())
    }

    fn verify_function(&self, fi: usize, f: &Function) -> Result<(), LoadError> {
        let err = |m: String| LoadError::Invalid(format!("function {fi} ({}): {m}", f.name));
        if (f.params as usize) > f.regs as usize && !(f.params == 0 && f.regs == 0) {
            return Err(err("params exceed regs".into()));
        }
        if f.file as usize >= self.units.len() {
            return Err(err("file index out of range".into()));
        }
        // Collect instruction boundaries
        let mut boundaries: Vec<u32> = Vec::new();
        let mut pc = 0usize;
        let code = &f.code;
        let mut last: Option<u8> = None;
        while pc < code.len() {
            boundaries.push(pc as u32);
            let (op0, _, _, _) = decode(code[pc]);
            let (wide, start) = if op0 == op::WIDE {
                if pc + 2 >= code.len() {
                    return Err(err("`Wide` without a following instruction".into()));
                }
                (true, pc + 1)
            } else {
                (false, pc)
            };
            let (opc, a8, b8, c8) = decode(code[start]);
            if opc == op::WIDE && wide {
                return Err(err("`Wide` after `Wide`".into()));
            }
            let info =
                op::info(opc).ok_or_else(|| err(format!("unknown opcode {opc} at {pc:04x}")))?;
            let (a, b, c) = if wide {
                let (_, ea, eb, ec) = decode(code[start + 1]);
                (a8 | (ea << 8), b8 | (eb << 8), c8 | (ec << 8))
            } else {
                (a8, b8, c8)
            };
            let bx = decode_bx(code[start], wide.then(|| code[start + 1]));
            let regs = f.regs as u32;
            let reg = |r: u32, what: &str| -> Result<(), LoadError> {
                if r >= regs {
                    Err(err(format!("register {what}={r} out of range at {pc:04x}")))
                } else {
                    Ok(())
                }
            };
            use op::Shape::*;
            match info.shape {
                None => {}
                A => reg(a, "A")?,
                AB => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                }
                ABC => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(c, "C")?;
                }
                ABCImm => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    if c > 8 {
                        return Err(err(format!("width {c} out of range at {pc:04x}")));
                    }
                }
                ABImm => {
                    reg(a, "A")?;
                    if b > 0 {
                        return Err(err(format!("error kind {b} out of range at {pc:04x}")));
                    }
                }
                ABRange2 => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 1, "B+1")?;
                }
                ABRange3 => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                    reg(b + 2, "B+2")?;
                }
                ABsC | ABsCImm => {
                    reg(a, "A")?;
                    reg(b, "B")?;
                }
                ASBxImm => reg(a, "A")?,
                ABx => {
                    reg(a, "A")?;
                    match opc {
                        op::LOAD_CONST => {
                            if bx as usize >= self.consts.len() {
                                return Err(err(format!("constant {bx} out of range at {pc:04x}")));
                            }
                        }
                        op::LOAD_GLOBAL | op::STORE_GLOBAL | op::MAKE_GLOBAL_REF => {
                            if bx as usize >= self.globals.len() {
                                return Err(err(format!("global {bx} out of range at {pc:04x}")));
                            }
                        }
                        op::CALL => {
                            let callee = self.funcs.get(bx as usize).ok_or_else(|| {
                                err(format!("function {bx} out of range at {pc:04x}"))
                            })?;
                            if a + callee.params as u32 > regs {
                                return Err(err(format!("call arguments exceed regs at {pc:04x}")));
                            }
                        }
                        op::CALL_HOST => {
                            let h = self.hosts.get(bx as usize).ok_or_else(|| {
                                err(format!("host function {bx} out of range at {pc:04x}"))
                            })?;
                            if a + h.params as u32 > regs {
                                return Err(err(format!(
                                    "host call arguments exceed regs at {pc:04x}"
                                )));
                            }
                        }
                        _ => {}
                    }
                }
                ASBx => reg(a, "A")?,
                SBx => {}
            }
            last = Some(opc);
            pc = if wide { start + 2 } else { start + 1 };
        }
        if pc != code.len() {
            return Err(err("instruction stream is truncated".into()));
        }
        if !matches!(
            last,
            Some(op::RET | op::RET_VOID | op::HALT | op::JMP | op::RAISE_ERROR)
        ) {
            return Err(err(
                "last instruction must be Ret, RetVoid, Halt, Jmp or RaiseError".into(),
            ));
        }
        // Jump targets
        for &start in &boundaries {
            let (opc0, _, _, _) = decode(code[start as usize]);
            let wide = opc0 == op::WIDE;
            let body = if wide { start + 1 } else { start };
            let (opc, _, b8, c8) = decode(code[body as usize]);
            let Some(info) = op::info(opc) else { continue };
            let next = if wide { start + 3 } else { start + 1 };
            let offset: i64 = match info.shape {
                op::Shape::SBx | op::Shape::ASBx => {
                    if wide {
                        let (_, _, eb, ec) = decode(code[body as usize + 1]);
                        let v = b8 | (c8 << 8) | (eb << 16) | (ec << 24);
                        v as i32 as i64
                    } else {
                        (b8 | (c8 << 8)) as u16 as i16 as i64
                    }
                }
                op::Shape::ABsC => {
                    if wide {
                        let (_, _, _, ec) = decode(code[body as usize + 1]);
                        ((c8 | (ec << 8)) as u16 as i16) as i64
                    } else {
                        (c8 as u8 as i8) as i64
                    }
                }
                _ => continue,
            };
            let target = next as i64 + offset;
            if target < 0
                || target as usize >= code.len()
                || boundaries.binary_search(&(target as u32)).is_err()
            {
                return Err(err(format!(
                    "jump target {target} is not an instruction boundary at {start:04x}"
                )));
            }
        }
        // Line table
        if f.lines.is_empty() || f.lines[0].0 != 0 {
            return Err(err("line table must start at 0".into()));
        }
        let mut prev = 0u32;
        for (i, (p, _)) in f.lines.iter().enumerate() {
            if (i > 0 && *p <= prev) || *p as usize >= code.len().max(1) {
                return Err(err(
                    "line table positions must be increasing and within the code".into(),
                ));
            }
            prev = *p;
        }
        for (r, _) in &f.locals {
            if *r as u32 >= f.regs as u32 {
                return Err(err("local name register out of range".into()));
            }
        }
        Ok(())
    }
}

// ---- Disassembly (chapter 5) ----

/// The `bytecode` dump.
pub fn disassemble(m: &Module) -> String {
    let mut out = String::new();
    out.push_str(&format!("; puyc format {FORMAT_VERSION}\n"));
    out.push_str(".const\n");
    for (i, c) in m.consts.iter().enumerate() {
        match c {
            ConstEntry::Int(v) => out.push_str(&format!("  {i}: int {v}\n")),
            ConstEntry::Str(s) => out.push_str(&format!(
                "  {i}: str {}\n",
                crate::lexer::debug_text(s.as_bytes())
            )),
            ConstEntry::Utf16(u) => out.push_str(&format!("  {i}: str16 {}\n", debug_utf16(u))),
        }
    }
    out.push_str(".global\n");
    for (i, g) in m.globals.iter().enumerate() {
        let kind = if g.kind == 0 { "int" } else { "str" };
        out.push_str(&format!("  {i}: {kind} {}\n", g.name));
    }
    out.push_str(".host\n");
    for (i, h) in m.hosts.iter().enumerate() {
        out.push_str(&format!(
            "  {i}: {} params={} ret={}\n",
            h.name, h.params, h.ret as u8
        ));
    }
    for (i, u) in m.units.iter().enumerate() {
        let idx = |v: Option<u32>| v.map(|x| x.to_string()).unwrap_or_else(|| "-".into());
        // Only the program row has an entry (M1 has one row)
        let entry = if i == 0 {
            m.entry.to_string()
        } else {
            "-".into()
        };
        out.push_str(&format!(
            ".unit {} file={} init={} fini={} entry={}\n",
            u.name,
            crate::lexer::debug_text(u.path.as_bytes()),
            idx(u.init),
            idx(u.fini),
            entry
        ));
    }
    for (i, f) in m.funcs.iter().enumerate() {
        out.push_str(&format!(
            ".func {i} {} params={} regs={}\n",
            f.name, f.params, f.regs
        ));
        let mut locals = f.locals.clone();
        locals.sort_by_key(|l| l.0);
        for (r, n) in &locals {
            out.push_str(&format!("  .local r{r} {n}\n"));
        }
        let mut li = 0usize;
        let mut pc = 0usize;
        while pc < f.code.len() {
            while li < f.lines.len() && f.lines[li].0 as usize <= pc {
                out.push_str(&format!("  .line {}\n", f.lines[li].1));
                li += 1;
            }
            let (text, len) = disassemble_one(&f.code, pc);
            out.push_str(&format!("  {pc:04x}  {text}\n"));
            pc += len;
        }
    }
    out
}

fn debug_utf16(u: &[u16]) -> String {
    let mut s = String::from("\"");
    for r in char::decode_utf16(u.iter().copied()) {
        match r {
            Ok(c) => {
                let d = format!("{:?}", c.to_string());
                s.push_str(&d[1..d.len() - 1]);
            }
            Err(e) => s.push_str(&format!("\\u{{{:x}}}", e.unpaired_surrogate())),
        }
    }
    s.push('"');
    s
}

/// A jump target position (4 hex digits; a negative value that fails verification is shown in decimal).
fn fmt_target(t: i64) -> String {
    if t < 0 {
        t.to_string()
    } else {
        format!("{t:04x}")
    }
}

/// Disassembles one instruction: (text, number of words).
pub fn disassemble_one(code: &[u32], pc: usize) -> (String, usize) {
    let (op0, _, _, _) = decode(code[pc]);
    let wide = op0 == op::WIDE;
    let body = if wide { pc + 1 } else { pc };
    let (opc, a8, b8, c8) = decode(code[body]);
    let (a, b, c) = if wide {
        let (_, ea, eb, ec) = decode(code[body + 1]);
        (a8 | (ea << 8), b8 | (eb << 8), c8 | (ec << 8))
    } else {
        (a8, b8, c8)
    };
    let len = if wide { 3 } else { 1 };
    let next = pc + len;
    let Some(info) = op::info(opc) else {
        return (format!("<unknown {opc}>"), len);
    };
    let name = if wide {
        format!("w.{}", info.name)
    } else {
        info.name.to_string()
    };
    let bx = decode_bx(code[body], wide.then(|| code[body + 1]));
    let sbx = if wide {
        bx as i32 as i64
    } else {
        (b8 | (c8 << 8)) as u16 as i16 as i64
    };
    let sc = if wide {
        ((c8 | ((c >> 8) << 8)) as u16 as i16) as i64
    } else {
        (c8 as u8 as i8) as i64
    };
    use op::Shape::*;
    let operands = match info.shape {
        None => String::new(),
        A => format!("r{a}"),
        AB | ABRange2 | ABRange3 => format!("r{a}, r{b}"),
        ABC => format!("r{a}, r{b}, r{c}"),
        ABCImm => format!("r{a}, r{b}, {c}"),
        ABImm => format!("r{a}, {b}"),
        ASBxImm => format!("r{a}, {sbx}"),
        ABx => {
            let prefix = match opc {
                op::LOAD_CONST => "k",
                op::LOAD_GLOBAL | op::STORE_GLOBAL | op::MAKE_GLOBAL_REF => "g",
                op::CALL => "f",
                op::CALL_HOST => "h",
                _ => "",
            };
            format!("r{a}, {prefix}{bx}")
        }
        ASBx => format!("r{a}, -> {}", fmt_target(next as i64 + sbx)),
        SBx => format!("-> {}", fmt_target(next as i64 + sbx)),
        ABsC => format!("r{a}, r{b}, -> {}", fmt_target(next as i64 + sc)),
        ABsCImm => format!("r{a}, r{b}, {sc}"),
    };
    if operands.is_empty() {
        (name, len)
    } else {
        (format!("{name:<12} {operands}"), len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Module {
        Module {
            consts: vec![
                ConstEntry::Str("Hello, world\n".into()),
                ConstEntry::Int(42),
                ConstEntry::Utf16(vec![0xD800]),
            ],
            funcs: vec![Function {
                name: "Hello.<main>".into(),
                params: 0,
                regs: 3,
                code: vec![
                    encode(op::LOAD_INT, 1, 0, 0),
                    encode(op::LOAD_CONST, 0, 0, 0),
                    encode(op::CALL_HOST, 0, 0, 0),
                    encode(op::RET_VOID, 0, 0, 0),
                ],
                lines: vec![(0, 5), (2, 6)],
                locals: vec![(1, "I".into())],
                file: 0,
            }],
            globals: vec![GlobalEntry {
                name: "Total".into(),
                kind: 0,
            }],
            entry: 0,
            units: vec![UnitEntry {
                name: "Hello".into(),
                path: "Hello.pas".into(),
                init: None,
                fini: None,
            }],
            hosts: vec![HostEntry {
                name: "Console.Write".into(),
                params: 1,
                ret: false,
            }],
        }
    }

    #[test]
    fn roundtrip() {
        let m = sample();
        let bytes = m.encode("0.0.0");
        assert_eq!(&bytes[..4], b"PUYC");
        let back = Module::decode(&bytes, "0.0.0").unwrap();
        assert_eq!(back, m);
        assert!(matches!(
            Module::decode(&bytes, "0.0.1"),
            Err(LoadError::Version { .. })
        ));
        assert!(matches!(
            Module::decode(b"XXXX", "0.0.0"),
            Err(LoadError::Invalid(_))
        ));
    }

    #[test]
    fn disassembly_format() {
        let d = disassemble(&sample());
        let expected = "\
; puyc format 1
.const
  0: str \"Hello, world\\n\"
  1: int 42
  2: str16 \"\\u{d800}\"
.global
  0: int Total
.host
  0: Console.Write params=1 ret=0
.unit Hello file=\"Hello.pas\" init=- fini=- entry=0
.func 0 Hello.<main> params=0 regs=3
  .local r1 I
  .line 5
  0000  LoadInt      r1, 0
  0001  LoadConst    r0, k0
  .line 6
  0002  CallHost     r0, h0
  0003  RetVoid
";
        assert_eq!(d, expected);
    }

    #[test]
    fn verify_rejects() {
        let mut m = sample();
        m.funcs[0].code[3] = encode(op::NOP, 0, 0, 0);
        assert!(m.verify().is_err());
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::MOVE, 5, 0, 0);
        assert!(m.verify().is_err());
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::JMP, 0, 7, 0);
        assert!(m.verify().is_err());
        let mut m = sample();
        m.funcs[0].code[0] = encode(op::JMP, 0, 1, 0);
        assert!(m.verify().is_ok());
    }

    #[test]
    fn wide_table_index() {
        let mut m = sample();
        m.consts.resize(65537, ConstEntry::Int(0));
        m.consts[65536] = ConstEntry::Int(42);
        m.funcs[0].code = vec![
            encode(op::WIDE, 0, 0, 0),
            encode(op::LOAD_CONST, 0, 0, 0),
            encode_ext(0, 256, 0),
            encode(op::RET, 0, 0, 0),
        ];
        m.funcs[0].lines = vec![(0, 1)];
        assert_eq!(
            disassemble_one(&m.funcs[0].code, 0).0,
            "w.LoadConst  r0, k65536"
        );
        assert_eq!(Module::decode(&m.encode("test"), "test").unwrap(), m);
        m.consts.truncate(65536);
        assert!(m.verify().is_err(), "verifies the whole 32-bit index");
    }

    #[test]
    fn wide_disassembly() {
        let code = vec![
            encode(op::WIDE, 0, 0, 0),
            encode(op::LOAD_CONST, 300, 0, 1),
            encode_ext(300, 0, 0),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        let (t, len) = disassemble_one(&code, 0);
        assert_eq!(len, 3);
        assert_eq!(t, "w.LoadConst  r300, k256");
        let code = vec![
            encode(op::JMP_IF_LT_INT, 1, 2, 0xFE),
            encode(op::RET_VOID, 0, 0, 0),
        ];
        let (t, _) = disassemble_one(&code, 0);
        assert_eq!(t, "JmpIfLtInt   r1, r2, -> -1");
    }
}
