//! The Rust parts of `System.RegularExpressions`: the unit `System.RegularExpressions.Native`,
//! which the Pascal unit uses (`docs/design/stdlib.md` 3.11; specification:
//! `docs/spec/stdlib/System.RegularExpressions/README.md`).
//!
//! The engine is the `regex` crate. It works on UTF-8 and gives byte offsets, while Pascal's
//! positions count UTF-16 code units: a string held as UTF-16 is converted once into a
//! [`Subject`] (a lone surrogate becomes U+FFFD, three bytes standing for one code unit), and the
//! offsets of its matches are mapped back by walking forward from the last mapped position, so
//! that a whole search costs O(length of the string). The text between the matches is copied
//! from the string's own code units, so that lone surrogates outside the matches are kept.

#[puy_ext::pascal_module(unit = "System.RegularExpressions.Native")]
pub mod native {
    use std::borrow::Cow;

    use puy_ext::prelude::*;
    use regex::{CaptureLocations, Regex, RegexBuilder};

    /// An invalid pattern, a position out of a string, a group that is not there.
    #[pascal_exception(name = "ERegularExpressionError")]
    pub struct RegexError;

    // The bits of the options, by the ordinals of `TRegExOption` (`roNone` is 0 and has no bit)
    const IGNORE_CASE: i32 = 1 << 1;
    const MULTI_LINE: i32 = 1 << 2;
    const EXPLICIT_CAPTURE: i32 = 1 << 3;
    const SINGLE_LINE: i32 = 1 << 5;
    const IGNORE_PATTERN_SPACE: i32 = 1 << 6;
    const NOT_EMPTY: i32 = 1 << 7;

    /// A compiled pattern and the groups Pascal sees.
    #[pascal_class(name = "TRegexEngine")]
    pub struct Engine {
        re: Regex,
        /// The engine's index of each group Pascal sees: all of them, or with
        /// `roExplicitCapture` the whole match and the named groups.
        groups: Vec<usize>,
        /// The names of the groups Pascal sees (`None` for an unnamed one).
        names: Vec<Option<String>>,
        not_empty: bool,
    }

    #[pascal_methods]
    impl Engine {
        /// Compiles `Pattern` with the options' bits; an invalid or unsupported pattern is an
        /// `ERegularExpressionError` with the engine's explanation.
        #[pascal_constructor]
        pub fn create(pattern: PasStr<'_>, options: i32) -> Result<Self, PasError> {
            let text = pattern.to_string();
            let re = RegexBuilder::new(&text)
                .case_insensitive(options & IGNORE_CASE != 0)
                .multi_line(options & MULTI_LINE != 0)
                .dot_matches_new_line(options & SINGLE_LINE != 0)
                .ignore_whitespace(options & IGNORE_PATTERN_SPACE != 0)
                .crlf(true)
                .build()
                .map_err(|e| RegexError::error(error_text(&e)))?;
            let explicit = options & EXPLICIT_CAPTURE != 0;
            let mut groups = Vec::new();
            let mut names = Vec::new();
            for (i, name) in re.capture_names().enumerate() {
                if i == 0 || !explicit || name.is_some() {
                    groups.push(i);
                    names.push(name.map(str::to_string));
                }
            }
            Ok(Engine {
                re,
                groups,
                names,
                not_empty: options & NOT_EMPTY != 0,
            })
        }

        /// The number of groups, the whole match (group 0) included.
        pub fn group_count(&self) -> i32 {
            count(self.groups.len())
        }

        /// The number of the group named `Name`, or -1.
        pub fn group_number(&self, name: PasStr<'_>) -> i32 {
            self.names
                .iter()
                .position(|n| n.as_deref().is_some_and(|n| name == *n))
                .map_or(-1, count)
        }

        /// Whether a match starts at or after the 0-based code unit `Start` of `Input`.
        pub fn is_match(&self, input: PasStr<'_>, start: i32) -> bool {
            let subject = Text::borrowed(input, input.len());
            let mut search = Search::new(self, &subject);
            let pos = search.text.to_byte(unit(start));
            search.next(pos, None)
        }

        /// The units `0..Stop` of `Input`, kept for the searches of a `TMatch`.
        pub fn subject(&self, input: PasStr<'_>, stop: i32) -> Subject {
            Subject {
                engine: self.clone_parts(),
                text: Text::owned(input, unit(stop)),
            }
        }

        /// `Input` with the first `Count` matches (all of them if `Count` is 0 or less) replaced
        /// by `Replacement` with its references expanded.
        pub fn replace(&self, input: PasStr<'_>, replacement: PasStr<'_>, count: i32) -> PasString {
            let template = Template::parse(replacement, self);
            let text = Text::borrowed(input, input.len());
            let mut search = Search::new(self, &text);
            let mut out = Vec::with_capacity(input.len());
            let mut copied = 0;
            let mut done = 0;
            let mut pos = 0;
            let mut last = None;
            while (count <= 0 || done < count) && search.next(pos, last) {
                let spans = search.spans();
                let (s, len) = (spans[0], spans[1]);
                text.push_units(&mut out, copied, s as usize);
                template.expand(&mut out, &spans, &text);
                copied = (s + len) as usize;
                done += 1;
                let (_, e) = search.locs.get(0).expect("group 0 takes part in a match");
                pos = e;
                last = Some(e);
            }
            text.push_units(&mut out, copied, input.len());
            PasString::from_utf16(out)
        }
    }

    impl Engine {
        fn clone_parts(&self) -> Engine {
            Engine {
                re: self.re.clone(),
                groups: self.groups.clone(),
                names: self.names.clone(),
                not_empty: self.not_empty,
            }
        }
    }

    /// The text a `TMatch` searches: the string up to the end of its range, converted once.
    #[pascal_class(name = "TRegexSubject")]
    pub struct Subject {
        engine: Engine,
        text: Text<'static>,
    }

    #[pascal_methods]
    impl Subject {
        /// The first match at or after the code unit `Start`; an empty match that ends at
        /// `After` is skipped (-1: none). The result is the start (0-based, -1 for a group that
        /// took no part) and the length of each group, or empty when there is no match.
        pub fn find(&self, start: i32, after: i32) -> Vec<i32> {
            let mut search = Search::new(&self.engine, &self.text);
            let pos = search.text.to_byte(unit(start));
            let last = (after >= 0).then(|| search.text.to_byte(unit(after)));
            if search.next(pos, last) {
                search.spans()
            } else {
                Vec::new()
            }
        }

        /// The matches at or after the code unit `Start`, at most `Limit` of them (all if
        /// `Limit` is 0 or less), one after the other in the form of `Find`.
        pub fn find_all(&self, start: i32, limit: i32) -> Vec<i32> {
            let mut search = Search::new(&self.engine, &self.text);
            let mut pos = search.text.to_byte(unit(start));
            let mut last = None;
            let mut out = Vec::new();
            let mut n = 0;
            while (limit <= 0 || n < limit) && search.next(pos, last) {
                out.extend(search.spans());
                let (_, e) = search.locs.get(0).expect("group 0 takes part in a match");
                pos = e;
                last = Some(e);
                n += 1;
            }
            out
        }

        /// `Replacement` expanded for the match `Spans` (the form of `Find`).
        pub fn expand(&self, spans: &[i32], replacement: PasStr<'_>) -> PasString {
            let template = Template::parse(replacement, &self.engine);
            let mut out = Vec::new();
            template.expand(&mut out, spans, &self.text);
            PasString::from_utf16(out)
        }
    }

    /// The explanation of a pattern the engine refused, on one line.
    fn error_text(e: &regex::Error) -> String {
        match e {
            regex::Error::Syntax(s) => {
                // The engine's message draws the pattern and a caret under the error; keep its
                // last line, the explanation
                let last = s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(s);
                let last = last.trim();
                let last = last.strip_prefix("error: ").unwrap_or(last);
                format!("invalid regular expression: {last}")
            }
            regex::Error::CompiledTooBig(_) => {
                "the regular expression is too large once compiled".to_string()
            }
            _ => format!("invalid regular expression: {e}"),
        }
    }

    fn count(n: usize) -> i32 {
        i32::try_from(n).expect("a string's length fits in Integer")
    }

    /// A 0-based position from Pascal, checked by the Pascal unit to be within the string.
    fn unit(i: i32) -> usize {
        usize::try_from(i.max(0)).expect("a non-negative i32 fits in usize")
    }

    /// The text searched: UTF-8 for the engine, and the code units it came from.
    struct Text<'a> {
        utf8: Cow<'a, str>,
        /// The code units when the string is held as UTF-16 (`None`: one byte per character,
        /// each byte a code unit).
        utf16: Option<Cow<'a, [u16]>>,
        /// The end of the range searched, in bytes of `utf8`.
        stop: usize,
        /// The last mapped position, `(byte, code unit)`, where the next mapping starts
        cursor: std::cell::Cell<(usize, usize)>,
    }

    impl<'a> Text<'a> {
        fn borrowed(s: PasStr<'a>, stop: usize) -> Text<'a> {
            match s {
                PasStr::Ascii(a) => Text::new(Cow::Borrowed(a), None, stop),
                PasStr::Utf16(u) => {
                    let utf8 = s.to_string();
                    Text::new(Cow::Owned(utf8), Some(Cow::Borrowed(u)), stop)
                }
            }
        }

        fn new(utf8: Cow<'a, str>, utf16: Option<Cow<'a, [u16]>>, stop: usize) -> Text<'a> {
            let mut t = Text {
                utf8,
                utf16,
                stop: 0,
                cursor: std::cell::Cell::new((0, 0)),
            };
            t.stop = t.to_byte_floor(stop);
            t.cursor.set((0, 0));
            t
        }

        /// The number of code units.
        fn len(&self) -> usize {
            self.utf16.as_ref().map_or(self.utf8.len(), |u| u.len())
        }

        /// The byte where the code unit `unit` is (the next character's when it is the second
        /// half of a surrogate pair).
        fn to_byte(&self, unit: usize) -> usize {
            if self.utf16.is_none() {
                return unit.min(self.utf8.len());
            }
            let (mut b, mut u) = self.cursor.get();
            if unit < u {
                (b, u) = (0, 0);
            }
            for c in self.utf8[b..].chars() {
                if u >= unit {
                    break;
                }
                b += c.len_utf8();
                u += c.len_utf16();
            }
            self.cursor.set((b, u));
            b
        }

        /// The byte of the last character boundary at or before the code unit `unit` (the end
        /// of a range: a range that ends between the halves of a surrogate pair stops before the
        /// pair).
        fn to_byte_floor(&self, unit: usize) -> usize {
            if self.utf16.is_none() {
                return unit.min(self.utf8.len());
            }
            let (mut b, mut u) = (0, 0);
            for c in self.utf8.chars() {
                if u + c.len_utf16() > unit {
                    break;
                }
                b += c.len_utf8();
                u += c.len_utf16();
            }
            b
        }

        /// The code unit where the byte `byte` (a character's boundary) is.
        fn to_unit(&self, byte: usize) -> usize {
            if self.utf16.is_none() {
                return byte;
            }
            let (mut b, mut u) = self.cursor.get();
            if byte < b {
                (b, u) = (0, 0);
            }
            for c in self.utf8[b..byte].chars() {
                u += c.len_utf16();
            }
            self.cursor.set((byte, u));
            u
        }

        /// Appends the code units `from..to`.
        fn push_units(&self, out: &mut Vec<u16>, from: usize, to: usize) {
            if from >= to {
                return;
            }
            match &self.utf16 {
                Some(u) => out.extend_from_slice(&u[from..to]),
                None => out.extend(self.utf8.as_bytes()[from..to].iter().map(|&b| u16::from(b))),
            }
        }
    }

    impl Text<'static> {
        fn owned(s: PasStr<'_>, stop: usize) -> Text<'static> {
            match s {
                PasStr::Ascii(a) => Text::new(Cow::Owned(a.to_string()), None, stop),
                PasStr::Utf16(u) => {
                    let utf8 = s.to_string();
                    Text::new(Cow::Owned(utf8), Some(Cow::Owned(u.to_vec())), stop)
                }
            }
        }
    }

    /// One search over a [`Text`], with the engine's places for the groups.
    struct Search<'e, 't, 'a> {
        engine: &'e Engine,
        text: &'t Text<'a>,
        locs: CaptureLocations,
    }

    impl<'e, 't, 'a> Search<'e, 't, 'a> {
        fn new(engine: &'e Engine, text: &'t Text<'a>) -> Self {
            Search {
                engine,
                text,
                locs: engine.re.capture_locations(),
            }
        }

        /// Finds the first match at or after the byte `pos`, skipping empty matches with
        /// `roNotEmpty` and an empty match at `last` (where the previous match ended).
        fn next(&mut self, mut pos: usize, last: Option<usize>) -> bool {
            let hay = &self.text.utf8[..self.text.stop];
            loop {
                if pos > hay.len() {
                    return false;
                }
                if self.engine.re.captures_read_at(&mut self.locs, hay, pos).is_none() {
                    return false;
                }
                let (s, e) = self.locs.get(0).expect("group 0 takes part in a match");
                if s == e && (self.engine.not_empty || Some(e) == last) {
                    // Go on from the next character
                    match hay[s..].chars().next() {
                        Some(c) => pos = s + c.len_utf8(),
                        None => return false,
                    }
                    continue;
                }
                return true;
            }
        }

        /// The start (0-based code unit, -1 when the group took no part) and the length of each
        /// group Pascal sees, in the last match found.
        fn spans(&self) -> Vec<i32> {
            let mut bounds: Vec<usize> = Vec::with_capacity(self.engine.groups.len() * 2);
            for &g in &self.engine.groups {
                if let Some((s, e)) = self.locs.get(g) {
                    bounds.push(s);
                    bounds.push(e);
                }
            }
            bounds.sort_unstable();
            bounds.dedup();
            let units: Vec<usize> = bounds.iter().map(|&b| self.text.to_unit(b)).collect();
            let at = |b: usize| units[bounds.binary_search(&b).expect("a bound mapped above")];
            let mut out = Vec::with_capacity(self.engine.groups.len() * 2);
            for &g in &self.engine.groups {
                match self.locs.get(g) {
                    Some((s, e)) => {
                        let (s, e) = (at(s), at(e));
                        out.push(count(s));
                        out.push(count(e - s));
                    }
                    None => {
                        out.push(-1);
                        out.push(0);
                    }
                }
            }
            out
        }
    }

    /// A replacement string read into its parts (`docs/spec/stdlib/System.RegularExpressions/
    /// README.md` 5).
    enum Part {
        Text(Vec<u16>),
        Group(usize),
        Before,
        After,
        Input,
    }

    struct Template(Vec<Part>);

    impl Template {
        fn parse(s: PasStr<'_>, engine: &Engine) -> Template {
            let units: Vec<u16> = s.units().collect();
            let groups = engine.groups.len();
            let mut parts = Vec::new();
            let mut lit = Vec::new();
            let mut i = 0;
            let digit = |u: u16| (u16::from(b'0')..=u16::from(b'9')).contains(&u);
            while i < units.len() {
                let c = units[i];
                let next = units.get(i + 1).copied();
                let part = if c == u16::from(b'$') || c == u16::from(b'\\') {
                    match next {
                        Some(n) if n == c => {
                            // `$$` and `\\`
                            lit.push(c);
                            i += 2;
                            continue;
                        }
                        Some(n) if digit(n) => {
                            let mut end = i + 1;
                            while end < units.len() && digit(units[end]) {
                                end += 1;
                            }
                            // The longest run of digits that numbers a group; with none, the
                            // first digit stands for a group that is not there (empty)
                            let mut take = end;
                            let mut group = None;
                            while take > i + 1 {
                                if let Some(g) = number(&units[i + 1..take]).filter(|&g| g < groups)
                                {
                                    group = Some(g);
                                    break;
                                }
                                take -= 1;
                            }
                            match group {
                                Some(g) => {
                                    i = take;
                                    Some(Part::Group(g))
                                }
                                None => {
                                    i += 2;
                                    Some(Part::Text(Vec::new()))
                                }
                            }
                        }
                        Some(n) if c == u16::from(b'$') => {
                            let simple = match u8::try_from(n).ok() {
                                Some(b'&') => Some(Part::Group(0)),
                                Some(b'`') => Some(Part::Before),
                                Some(b'\'') => Some(Part::After),
                                Some(b'_') => Some(Part::Input),
                                // The last group; none without groups
                                Some(b'+') if groups > 1 => Some(Part::Group(groups - 1)),
                                Some(b'+') => Some(Part::Text(Vec::new())),
                                _ => None,
                            };
                            if let Some(p) = simple {
                                i += 2;
                                Some(p)
                            } else if n == u16::from(b'{') {
                                let close = units[i + 2..]
                                    .iter()
                                    .position(|&u| u == u16::from(b'}'))
                                    .map(|p| i + 2 + p);
                                let g = close.and_then(|close| {
                                    let name = &units[i + 2..close];
                                    let g = number(name).filter(|&g| g < groups).or_else(|| {
                                        let name = String::from_utf16(name).ok()?;
                                        engine
                                            .names
                                            .iter()
                                            .position(|n| n.as_deref() == Some(name.as_str()))
                                    })?;
                                    Some((g, close))
                                });
                                match g {
                                    Some((g, close)) => {
                                        i = close + 1;
                                        Some(Part::Group(g))
                                    }
                                    None => None,
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                match part {
                    Some(p) => {
                        if !lit.is_empty() {
                            parts.push(Part::Text(std::mem::take(&mut lit)));
                        }
                        parts.push(p);
                    }
                    None => {
                        lit.push(c);
                        i += 1;
                    }
                }
            }
            if !lit.is_empty() {
                parts.push(Part::Text(lit));
            }
            Template(parts)
        }

        /// Appends the replacement of the match `spans` of `text`.
        fn expand(&self, out: &mut Vec<u16>, spans: &[i32], text: &Text<'_>) {
            let span = |g: usize| -> Option<(usize, usize)> {
                let s = *spans.get(g * 2)?;
                let len = *spans.get(g * 2 + 1)?;
                let s = usize::try_from(s).ok()?;
                Some((s, s + usize::try_from(len).ok()?))
            };
            for part in &self.0 {
                match part {
                    Part::Text(t) => out.extend_from_slice(t),
                    Part::Group(g) => {
                        if let Some((s, e)) = span(*g) {
                            text.push_units(out, s, e);
                        }
                    }
                    Part::Before => {
                        if let Some((s, _)) = span(0) {
                            text.push_units(out, 0, s);
                        }
                    }
                    Part::After => {
                        if let Some((_, e)) = span(0) {
                            text.push_units(out, e, text.len());
                        }
                    }
                    Part::Input => text.push_units(out, 0, text.len()),
                }
            }
        }
    }

    /// The decimal number of the digits `d`, or `None` when it is too large to be a group's.
    fn number(d: &[u16]) -> Option<usize> {
        if d.is_empty() || d.len() > 6 {
            return None;
        }
        let mut n = 0usize;
        for &u in d {
            let v = u.checked_sub(u16::from(b'0')).filter(|&v| v <= 9)?;
            n = n * 10 + usize::from(v);
        }
        Some(n)
    }
}
