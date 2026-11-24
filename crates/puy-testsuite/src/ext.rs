//! The extensions the test cases use (`docs/design/testing.md` 3.7): `TestExt`, a small
//! extension written with the attributes of `puy-ext` (`docs/spec/extensions.md` chapter 7) that
//! exercises the calls, the values, the exceptions, the classes and the callbacks of the
//! extension mechanism, and `TestExtBad`, a registration table written by hand whose
//! declarations break the rules of `native` (for the diagnostics E0771–E0773; the attributes
//! cannot write such declarations).

use puy_core::ext::{Extension, ExtensionSet, fingerprint};

pub use test_ext::{EXTENSION as TEST_EXT, SOURCE};

/// The extensions of the test runtime: the standard library's, as every runtime registers them,
/// and the test extensions.
pub fn extensions() -> ExtensionSet {
    puy_stdlib_ext::extensions()
        .with(&TEST_EXT)
        .with(&TEST_EXT_BAD)
}

#[puy_ext::pascal_module(unit = "TestExt")]
mod test_ext {
    use std::cell::Cell;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use puy_ext::prelude::*;

    /// The largest count the tests use.
    #[pascal_const]
    pub const TEST_LIMIT: i32 = 100;

    #[pascal_const(name = "TestGreeting")]
    pub const GREETING: &str = "it's\tok";

    #[pascal_exception(name = "ETestError")]
    pub struct TestError;

    /// An exception whose parent is a built-in class.
    #[pascal_exception(name = "ETestRange", parent = ERangeError)]
    pub struct TestRange;

    #[pascal_enum(name = "TColor")]
    pub enum Color {
        Red,
        Green,
        #[pascal(name = "Azure")]
        Blue,
    }

    /// A part of a string: where it starts (0-based, in UTF-16 code units) and its length.
    #[pascal_record(name = "TSpan")]
    pub struct Span {
        pub start: i64,
        pub length: i32,
    }

    /// A record whose fields are of every kind a record can hold: kept as a block in its
    /// containers, as it takes more than 16 slots once its `TSpan` is laid out flat in it.
    #[pascal_record(name = "TEntry")]
    pub struct Entry {
        name: String,
        size: f64,
        ratio: f32,
        color: Color,
        initial: PasChar,
        ok: bool,
        tag: Utf8String,
        data: Vec<u8>,
        spans: Vec<Span>,
        first: Span,
        n1: u8,
        n2: i16,
        n3: u16,
        n4: u32,
        n5: u64,
        n6: i8,
        n7: char,
    }

    #[pascal_callback(name = "TIntFunc")]
    pub type IntFunc = fn(x: i32) -> i32;

    #[pascal_callback(name = "TStrFunc")]
    pub type StrFunc = fn(s: &str) -> String;

    #[pascal_callback(name = "TIntProc")]
    pub type IntProc = fn(x: i32);

    thread_local! {
        /// The number of [`Counter`]s alive on this thread (each case runs on a thread of its
        /// own).
        static LIVE: Cell<i32> = const { Cell::new(0) };
    }

    /// A counter whose state is a Rust value.
    #[pascal_class(name = "TCounter")]
    pub struct Counter {
        value: i32,
    }

    #[pascal_methods]
    impl Counter {
        fn new(value: i32) -> Counter {
            LIVE.with(|l| l.set(l.get() + 1));
            Counter { value }
        }

        #[pascal_constructor]
        pub fn create(start: i32) -> Self {
            Counter::new(start)
        }

        pub fn increment(&mut self) {
            self.value += 1;
        }

        #[pascal_function(getter = "Count")]
        pub fn value(&self) -> i32 {
            self.value
        }

        #[pascal_function(setter = "Count")]
        pub fn set_count(&mut self, v: i32) {
            self.value = v;
        }

        /// Calls `F` with each value from 1 to `Value`, holding the counter while it runs.
        pub fn each(&mut self, ctx: &mut CallCtx<'_>, f: PasFn<IntProc>) -> Result<(), PasError> {
            // Held mutably while the callback runs: a callback that touches the counter
            // re-enters it
            for i in 1..=self.value {
                ctx.call(&f, vec![Value::from(i)])?;
            }
            Ok(())
        }

        pub fn same(this: PasObj<Self>) -> PasObj<Self> {
            this
        }

        /// The number of counters whose Rust value is alive.
        pub fn live() -> i32 {
            LIVE.with(Cell::get)
        }
    }

    impl Drop for Counter {
        fn drop(&mut self) {
            LIVE.with(|l| l.set(l.get() - 1));
        }
    }

    /// Holds a callable value in its Rust value, and calls it.
    #[pascal_class(name = "THolder")]
    pub struct Holder {
        f: PasFn<IntFunc>,
    }

    #[pascal_methods]
    impl Holder {
        #[pascal_constructor]
        pub fn create(f: PasFn<IntFunc>) -> Self {
            Holder { f }
        }

        pub fn call(&self, ctx: &mut CallCtx<'_>, x: i32) -> Result<i32, PasError> {
            int_result(ctx.call(&self.f, vec![Value::from(x)])?)
        }
    }

    /// A class without a constructor: its instances come from `NewTicket`.
    #[pascal_class(name = "TTicket")]
    pub struct Ticket(i32);

    #[pascal_methods]
    impl Ticket {
        pub fn id(&self) -> i32 {
            self.0
        }
    }

    /// The `Integer` a callback returned.
    fn int_result(v: Value) -> Result<i32, PasError> {
        let v = v
            .as_int()
            .ok_or_else(|| PasError::invalid_operation("not an ordinal"))?;
        i32::try_from(v).map_err(|_| PasError::range_error("out of range"))
    }

    #[pascal_function]
    pub fn add(a: i32, b: i32) -> Result<i32, PasError> {
        a.checked_add(b)
            .ok_or_else(|| PasError::new("EIntOverflow", "Add overflowed"))
    }

    #[pascal_function]
    pub fn add_int64(a: i64, b: i64) -> i64 {
        a.wrapping_add(b)
    }

    #[pascal_function]
    pub fn half(x: f64) -> f64 {
        x / 2.0
    }

    #[pascal_function]
    pub fn half_single(x: f32) -> f32 {
        x / 2.0
    }

    #[pascal_function]
    pub fn negate(b: bool) -> bool {
        !b
    }

    /// The next `Char` (a lone surrogate is out of range).
    #[pascal_function]
    pub fn next_char(c: PasChar) -> Result<PasChar, PasError> {
        let ch = c
            .to_char()
            .ok_or_else(|| PasError::range_error(format!("#{} is not a character", c.0)))?;
        Ok(char::from_u32(ch as u32 + 1)
            .and_then(PasChar::from_char)
            .unwrap_or(c))
    }

    /// The next Unicode scalar value (`UCS4Char`).
    #[pascal_function]
    pub fn next_scalar(c: char) -> char {
        char::from_u32(c as u32 + 1).unwrap_or(c)
    }

    #[pascal_function]
    pub fn next_color(c: Color) -> Color {
        match c {
            Color::Red => Color::Green,
            Color::Green => Color::Blue,
            Color::Blue => Color::Red,
        }
    }

    #[pascal_function]
    pub fn fail_test_range() -> Result<(), PasError> {
        Err(TestRange::error("too far"))
    }

    #[pascal_function]
    pub fn inc1(x: i32) -> i32 {
        x + 1
    }

    /// Upper-cases ASCII letters without converting the string (`PasStr` in, `PasString` out).
    #[pascal_function]
    pub fn upper(s: PasStr<'_>) -> PasString {
        let units: Vec<u16> = s
            .units()
            .map(|u| {
                if (u16::from(b'a')..=u16::from(b'z')).contains(&u) {
                    u - 32
                } else {
                    u
                }
            })
            .collect();
        PasString::from_utf16(units)
    }

    #[pascal_function]
    pub fn utf8_len(s: &str) -> i32 {
        s.len() as i32
    }

    #[pascal_function]
    pub fn utf16_len(s: PasStr<'_>) -> i32 {
        s.len() as i32
    }

    #[pascal_function]
    pub fn utf8_echo(s: Utf8String) -> Utf8String {
        Utf8String(format!("<{}>", s.0))
    }

    #[pascal_function]
    pub fn checksum(b: &[u8]) -> i32 {
        b.iter().map(|x| i32::from(*x)).sum()
    }

    #[pascal_function]
    pub fn reverse(b: &[u8]) -> Vec<u8> {
        b.iter().rev().copied().collect()
    }

    #[pascal_function]
    pub fn fail(msg: &str) -> Result<(), PasError> {
        Err(TestError::error(msg))
    }

    #[pascal_function(name = "FailIO")]
    pub fn fail_io(code: i32) -> Result<(), PasError> {
        Err(PasError::in_out("disk on fire", i64::from(code)))
    }

    #[pascal_function]
    pub fn fail_range() -> Result<(), PasError> {
        Err(PasError::range_error("out of the test range"))
    }

    #[pascal_function]
    pub fn apply(ctx: &mut CallCtx<'_>, f: PasFn<IntFunc>, x: i32) -> Result<i32, PasError> {
        int_result(ctx.call(&f, vec![Value::from(x)])?)
    }

    #[pascal_function]
    pub fn apply_str(
        ctx: &mut CallCtx<'_>,
        f: PasFn<StrFunc>,
        s: &str,
    ) -> Result<String, PasError> {
        let r = ctx.call(&f, vec![Value::from(s)])?;
        Ok(format!("[{}]", r.into_string().unwrap_or_default()))
    }

    /// Calls `F(X)`; an exception of the callback is caught here, and the result is then -1.
    #[pascal_function]
    pub fn try_apply(ctx: &mut CallCtx<'_>, f: PasFn<IntFunc>, x: i32) -> Result<i32, PasError> {
        match ctx.call(&f, vec![Value::from(x)]) {
            Ok(v) => int_result(v),
            Err(PasError::Halt) => Err(PasError::Halt),
            Err(_) => Ok(-1),
        }
    }

    /// Calls `F` with two arguments, one more than its signature takes (`EArgumentException`).
    #[pascal_function]
    pub fn apply_two(ctx: &mut CallCtx<'_>, f: PasFn<IntFunc>) -> Result<i32, PasError> {
        int_result(ctx.call(&f, vec![Value::from(1), Value::from(2)])?)
    }

    /// Sets the environment variable `name` for the isolate through `CallCtx::set_env_var`
    /// (`EArgumentException` for a name it refuses).
    #[pascal_function]
    pub fn set_env(ctx: &mut CallCtx<'_>, name: &str, value: &str) -> Result<(), PasError> {
        ctx.set_env_var(name, Some(value))
    }

    /// A future that is not done the first time it is polled.
    struct Later(bool);

    impl Future for Later {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            if !self.0 {
                self.0 = true;
                return Poll::Pending;
            }
            Poll::Ready(())
        }
    }

    #[pascal_function]
    pub async fn slow_add(a: i32, b: i32) -> i32 {
        Later(false).await;
        a + b
    }

    #[pascal_function(cancel = "complete")]
    pub async fn slow_fail() -> Result<i32, PasError> {
        Later(false).await;
        Err(TestError::error("failed later"))
    }

    #[pascal_function]
    pub fn maybe_counter(make: bool) -> Option<Counter> {
        make.then(|| Counter::new(42))
    }

    #[pascal_function]
    pub fn counter_value(c: Option<PasObj<Counter>>) -> Result<i32, PasError> {
        match c {
            Some(c) => Ok(c.borrow()?.value),
            None => Ok(-1),
        }
    }

    #[pascal_function]
    pub fn new_ticket(id: i32) -> Ticket {
        Ticket(id)
    }

    #[pascal_function]
    pub fn shift_span(s: Span, by: i64) -> Span {
        Span {
            start: s.start + by,
            length: s.length,
        }
    }

    /// The spans of the runs of `c` in `s`.
    #[pascal_function]
    pub fn runs_of(s: PasStr<'_>, c: PasChar) -> Vec<Span> {
        let mut out: Vec<Span> = Vec::new();
        for (i, u) in s.units().enumerate() {
            if u != c.0 {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.start + i64::from(last.length) == i as i64 => last.length += 1,
                _ => out.push(Span {
                    start: i as i64,
                    length: 1,
                }),
            }
        }
        out
    }

    #[pascal_function]
    pub fn sum_ints(a: &[i32]) -> i64 {
        a.iter().map(|&x| i64::from(x)).sum()
    }

    #[pascal_function]
    pub fn count_up(n: i32) -> Vec<i32> {
        (1..=n).collect()
    }

    #[pascal_function]
    pub fn upper_all(a: Vec<String>) -> Vec<PasString> {
        a.iter()
            .map(|s| PasString::from(s.to_uppercase()))
            .collect()
    }

    #[pascal_function]
    pub fn transpose(m: Vec<Vec<f64>>) -> Vec<Vec<f64>> {
        let cols = m.first().map_or(0, Vec::len);
        (0..cols)
            .map(|j| {
                m.iter()
                    .map(|row| row.get(j).copied().unwrap_or(0.0))
                    .collect()
            })
            .collect()
    }

    /// The bytes split at each 0.
    #[pascal_function]
    pub fn split_bytes(b: &[u8]) -> Vec<Vec<u8>> {
        b.split(|&x| x == 0).map(<[u8]>::to_vec).collect()
    }

    #[pascal_function]
    pub fn rainbow() -> Vec<Color> {
        vec![Color::Red, Color::Green, Color::Blue]
    }

    #[pascal_function]
    pub fn describe(e: Entry) -> String {
        let spans: Vec<String> = e
            .spans
            .iter()
            .map(|s| format!("{}+{}", s.start, s.length))
            .collect();
        format!(
            "{} {} {} {} {:?} {} {} {:?} [{}] {}+{} {} {} {} {} {} {} {:?}",
            e.name,
            e.size,
            e.ratio,
            e.color as i32,
            e.initial.to_char(),
            e.ok,
            e.tag.0,
            e.data,
            spans.join(" "),
            e.first.start,
            e.first.length,
            e.n1,
            e.n2,
            e.n3,
            e.n4,
            e.n5,
            e.n6,
            e.n7
        )
    }

    #[pascal_function]
    pub fn make_entries(names: &[Utf8String]) -> Vec<Entry> {
        names
            .iter()
            .enumerate()
            .map(|(i, n)| Entry {
                name: n.0.clone(),
                size: i as f64 + 0.5,
                ratio: 0.25,
                color: Color::Blue,
                initial: PasChar(u16::from(b'A') + i as u16),
                ok: i % 2 == 0,
                tag: n.clone(),
                data: vec![i as u8; i],
                spans: (0..i as i64)
                    .map(|k| Span {
                        start: k * 10,
                        length: 1,
                    })
                    .collect(),
                first: Span {
                    start: i64::MAX,
                    length: -1,
                },
                n1: 255,
                n2: -32768,
                n3: 65535,
                n4: u32::MAX,
                n5: u64::MAX,
                n6: -128,
                n7: '😀',
            })
            .collect()
    }

    /// The results of `f` on each element (copied before the calls back).
    #[pascal_function]
    pub fn map_ints(
        ctx: &mut CallCtx<'_>,
        f: PasFn<IntFunc>,
        a: &[i32],
    ) -> Result<Vec<i32>, PasError> {
        a.iter()
            .map(|&x| int_result(ctx.call(&f, vec![Value::Int(i64::from(x))])?))
            .collect()
    }

    #[pascal_function]
    pub async fn sum_later(a: Vec<i64>) -> i64 {
        Later(false).await;
        a.iter().sum()
    }
}

const BAD_SOURCE: &str = r#"unit TestExtBad;

interface

type
  TPoint = record
    X, Y: Integer;
    Corners: array[0..1] of Integer;
  end;

  TNode = record
    Kids: TArray<TNode>;
  end;

  // Each level doubles the fields of the shape: `TWide12` has 12287 nodes, more than a shape may
  // have; `TWide10` has 3071
  TWide0 = record A: Integer; end;
  TWide1 = record A, B: TWide0; end;
  TWide2 = record A, B: TWide1; end;
  TWide3 = record A, B: TWide2; end;
  TWide4 = record A, B: TWide3; end;
  TWide5 = record A, B: TWide4; end;
  TWide6 = record A, B: TWide5; end;
  TWide7 = record A, B: TWide6; end;
  TWide8 = record A, B: TWide7; end;
  TWide9 = record A, B: TWide8; end;
  TWide10 = record A, B: TWide9; end;
  TWide11 = record A, B: TWide10; end;
  TWide12 = record A, B: TWide11; end;

  TOpen = class(TRefObject)
  public
    Count: Integer;
    procedure Run; native;
    procedure Plain;
    procedure Spin; virtual; native;
  end;

  TByRef = reference to procedure(var X: Integer);
  TArrayFunc = reference to function(const A: TArray<Integer>): Integer;

procedure Twice(var X: Integer); native;
procedure TakePoint(const P: TPoint); native;
procedure TakeByRef(const F: TByRef); native;
function MakePoint: TPoint; native;
procedure TakeObjects(const A: TArray<TRefObject>); native;
procedure TakeNode(const N: TNode); native;
procedure TakeWide(const W: TWide12); native;
procedure TakeNarrow(const W: TWide10); native;
function TakeArrays(const F: TArrayFunc): Integer; native;
procedure Same(A: Integer); overload; native;
procedure Same(A: string); overload; native;
procedure Later; forward; native;
procedure Bodied; native;
procedure Again;

implementation

procedure Hidden; forward; native;

procedure Again; native;

procedure TOpen.Plain;
begin
end;

procedure Bodied;
begin
end;

procedure Later;
begin
end;

end.
"#;

/// An extension whose declarations break the rules of `native` (E0771–E0773).
pub static TEST_EXT_BAD: Extension = Extension {
    unit: "TestExtBad",
    source: BAD_SOURCE,
    fingerprint: fingerprint(BAD_SOURCE),
    functions: &[],
    classes: &[],
    capabilities: &[],
};
