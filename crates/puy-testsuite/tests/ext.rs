//! Extension tests that cannot be cases (`docs/spec/extensions.md` 4): binding a module's
//! extensions when the isolate is made (F0007), and the extension sections of a module that no
//! compiler writes.

use std::sync::Arc;

use puy_core::bytecode::{Module, ext_kind};
use puy_core::driver::{CompileOptions, compile};
use puy_core::ext::{Class, Extension, ExtensionSet, Function, PasError, Value, fingerprint};
use puy_core::source::MemorySourceProvider;
use puy_core::vm::{HashSeed, Isolate, IsolateConfig, IsolateError};
use puy_testsuite::ext::TEST_EXT;

const PROGRAM: &str = "program P;\nuses TestExt;\nvar C: TCounter;\nbegin\n  C := TCounter.Create(Add(1, 2));\nend.\n";

/// The module of `src`, compiled with the test extensions.
fn module(src: &str) -> Module {
    let opts = CompileOptions {
        extensions: puy_testsuite::ext::extensions(),
        ..Default::default()
    };
    let mut provider = MemorySourceProvider::new();
    let out = compile("p.pas", src.as_bytes().to_vec(), &mut provider, &opts);
    let errors: Vec<&str> = out.diagnostics.iter().map(|d| d.message.as_str()).collect();
    out.stages
        .module
        .unwrap_or_else(|| panic!("compiled: {errors:?}"))
}

/// Makes an isolate of `module` whose runtime has the extensions `set`.
fn isolate(module: Module, set: ExtensionSet) -> Result<Isolate, IsolateError> {
    let config = IsolateConfig {
        hash_seed: HashSeed::Fixed(0),
        extensions: set,
        ..Default::default()
    };
    Isolate::new(Arc::new(module), puy_host::HostSet::empty(), config)
}

/// The message of the F0007 of making an isolate of [`PROGRAM`] with the extensions `set`.
fn binding_error(set: ExtensionSet) -> String {
    match isolate(module(PROGRAM), set) {
        Err(IsolateError::Extension(m)) => m,
        Err(e) => panic!("not F0007: {e}"),
        Ok(_) => panic!("the isolate was made"),
    }
}

fn leak(ext: Extension) -> &'static Extension {
    Box::leak(Box::new(ext))
}

fn nothing(_: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    Ok(Value::Nothing)
}

// With the extension of the unit it was compiled with, the isolate is made.
#[test]
fn binds_to_the_extension_it_was_compiled_with() {
    assert!(isolate(module(PROGRAM), puy_testsuite::ext::extensions()).is_ok());
}

// A runtime without the extension unit refuses the module when the isolate is made, not when
// the call runs.
#[test]
fn missing_extension_unit_is_f0007() {
    assert_eq!(
        binding_error(ExtensionSet::new()),
        "extension unit `TestExt` is not in this runtime"
    );
}

// An extension unit whose declarations changed since the compilation (another fingerprint) is
// refused, so that old bytecode never calls functions whose arguments moved.
#[test]
fn changed_extension_unit_is_f0007() {
    let changed = leak(Extension {
        fingerprint: fingerprint("unit TestExt; changed"),
        ..TEST_EXT
    });
    assert_eq!(
        binding_error(ExtensionSet::new().with(changed)),
        "extension unit `TestExt` differs from the one the program was compiled with"
    );
}

// A function or a class of the module that the extension does not register is refused.
#[test]
fn missing_function_or_class_is_f0007() {
    let no_add = leak(Extension {
        functions: Box::leak(Box::new([Function::sync("TCounter.Create", nothing)])),
        ..TEST_EXT
    });
    assert_eq!(
        binding_error(ExtensionSet::new().with(no_add)),
        "extension function `TestExt.Add` is not in this runtime"
    );
    let no_class = leak(Extension {
        classes: Box::leak(Box::new([Class::of::<u8>("TTicket")])),
        ..TEST_EXT
    });
    assert_eq!(
        binding_error(ExtensionSet::new().with(no_class)),
        "extension class `TestExt.TCounter` is not in this runtime"
    );
}

// The extension sections are written to `.puyc` and read back as they were.
#[test]
fn extension_sections_round_trip() {
    let m = module(PROGRAM);
    assert!(!m.exts.is_empty() && !m.ext_units.is_empty());
    let back = Module::decode(&m.encode(puy_core::VERSION), puy_core::VERSION).expect("decodes");
    assert_eq!(back, m);
}

// The verification refuses extension entries that point outside their tables or have kinds the
// VM does not know (`docs/spec/bytecode.md` 3.3).
#[test]
fn verification_refuses_bad_extension_entries() {
    let m = module(PROGRAM);
    type Change = Box<dyn Fn(&mut Module)>;
    let cases: Vec<(&str, Change)> = vec![
        ("unit", Box::new(|m| m.exts[0].unit = 9)),
        (
            "kind",
            Box::new(|m| m.exts[0].params[0] = ext_kind::LAST + 1),
        ),
        (
            "none param",
            Box::new(|m| m.exts[0].params[0] = ext_kind::NONE),
        ),
        (
            "result type",
            Box::new(|m| {
                let e = m.exts.iter_mut().find(|e| e.ret == ext_kind::INT).unwrap();
                e.ret_type = 64;
            }),
        ),
        (
            "signature",
            Box::new(|m| m.exts[0].sigs.push(Default::default())),
        ),
        ("class", Box::new(|m| m.ext_units[0].classes[0].1 = 7)),
    ];
    for (what, change) in cases {
        let mut bad = m.clone();
        change(&mut bad);
        assert!(bad.verify().is_err(), "{what}");
    }
}

/// The module of a program that passes records and arrays to the test extension.
fn shaped_module() -> Module {
    module(
        "program P;\nuses TestExt;\nvar S: TSpan;\nbegin\n  S := ShiftSpan(S, 1);\n  \
         WriteLn(Length(MakeEntries(['x'])));\nend.\n",
    )
}

// The shapes of records and arrays are written to `.puyc` and read back as they were.
#[test]
fn extension_shapes_round_trip() {
    let m = shaped_module();
    assert!(m.exts.iter().any(|e| !e.shapes.is_empty()));
    let back = Module::decode(&m.encode(puy_core::VERSION), puy_core::VERSION).expect("decodes");
    assert_eq!(back, m);
}

// Reading a `.puyc` refuses shapes nested deeper than the compiler makes them, before they
// are verified (the reader of a nested shape takes Rust stack).
#[test]
fn decoding_refuses_shapes_nested_too_deep() {
    use puy_core::bytecode::{ExtShape, FULL_RANGE, MAX_EXT_SHAPE_DEPTH, NONE_INDEX};
    let mut m = shaped_module();
    let mut deep = ExtShape::leaf(ext_kind::INT, NONE_INDEX, FULL_RANGE);
    for _ in 0..MAX_EXT_SHAPE_DEPTH {
        deep = ExtShape {
            kind: ext_kind::ARRAY,
            ty: NONE_INDEX,
            range: FULL_RANGE,
            items: vec![deep],
        };
    }
    m.exts[0].shapes.push(deep);
    let err = Module::decode(&m.encode(puy_core::VERSION), puy_core::VERSION)
        .expect_err("refused")
        .to_string();
    assert!(err.contains("nested too deep"), "{err}");
}

// The verification refuses shapes that do not match the kinds of their entry or the slots of
// their types (`docs/spec/bytecode.md` 3.3): the VM copying a value of a shape reads and writes
// slots as the shape says.
#[test]
fn verification_refuses_bad_extension_shapes() {
    fn shift(m: &mut Module) -> &mut ExtEntry {
        m.exts
            .iter_mut()
            .find(|e| e.name == "ShiftSpan")
            .expect("ShiftSpan")
    }
    /// The shape of `TEntry` in the result of `MakeEntries` (an array of them).
    fn entries(m: &mut Module) -> &mut ExtShape {
        let e = m
            .exts
            .iter_mut()
            .find(|e| e.name == "MakeEntries")
            .expect("MakeEntries");
        &mut e.shapes[1].items[0]
    }
    use puy_core::bytecode::{ExtEntry, ExtShape};
    let m = shaped_module();
    assert!(m.verify().is_ok());
    type Change = Box<dyn Fn(&mut Module)>;
    let cases: Vec<(&str, Change)> = vec![
        (
            "missing shape",
            Box::new(|m| {
                shift(m).shapes.pop();
            }),
        ),
        (
            "extra shape",
            Box::new(|m| {
                let s = shift(m).shapes[0].clone();
                shift(m).shapes.push(s);
            }),
        ),
        (
            "kind of the entry",
            Box::new(|m| {
                shift(m).shapes[0].kind = ext_kind::ARRAY;
            }),
        ),
        (
            "record type",
            Box::new(|m| {
                let ty = entries(m).ty;
                shift(m).shapes[0].ty = ty + 1;
            }),
        ),
        (
            "missing field",
            Box::new(|m| {
                shift(m).shapes[0].items.pop();
            }),
        ),
        (
            "plain field in a reference slot",
            Box::new(|m| {
                entries(m).items[0].kind = ext_kind::INT;
            }),
        ),
        (
            "reference field in a plain slot",
            Box::new(|m| {
                entries(m).items[1].kind = ext_kind::STRING;
                entries(m).items[1].range = puy_core::bytecode::FULL_RANGE;
            }),
        ),
        (
            "object field",
            Box::new(|m| {
                entries(m).items[0].kind = ext_kind::OBJECT;
            }),
        ),
        (
            "bad range",
            Box::new(|m| {
                entries(m).items[3].range = (1, 0);
            }),
        ),
        (
            "bytes type",
            Box::new(|m| {
                let ty = entries(m).ty;
                entries(m).items[7].ty = ty;
            }),
        ),
        (
            "array without element",
            Box::new(|m| {
                entries(m).items[8].items.clear();
            }),
        ),
    ];
    for (what, change) in cases {
        let mut bad = m.clone();
        change(&mut bad);
        assert!(bad.verify().is_err(), "{what}");
    }
}

const BAD_SOURCE: &str = r#"unit BadExt;

interface

type
  TByteFunc = reference to function(B: Byte): Integer;
  TPair = record
    A: Byte;
    B: string;
  end;

function Big: Byte; native;
function StrAsObj(const S: string): TRefObject; native;
procedure RaiseString(const S: string); native;
function CallWide(const F: TByteFunc): Integer; native;
function BigInRecord: TPair; native;
function ShortRecord: TPair; native;
function NotArray: TArray<Integer>; native;
function BigInArray: TArray<TPair>; native;
function ArrayForRecord: TPair; native;

implementation

end.
"#;

fn big(_: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    Ok(Value::Int(1000))
}

fn str_as_obj(ctx: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    Ok(Value::from(ctx.reference(0)?))
}

fn raise_string(ctx: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    Err(PasError::Pascal(ctx.reference(0)?.expect("a string")))
}

fn call_wide(ctx: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    let f = ctx.func(0)?.expect("a function");
    ctx.call(&f, vec![Value::Int(300)])
}

fn big_in_record(_: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    Ok(Value::Record(vec![
        Value::Int(1000),
        Value::Str("x".into()),
    ]))
}

fn short_record(_: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    Ok(Value::Record(vec![Value::Int(1)]))
}

fn not_array(_: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    Ok(Value::Int(1))
}

fn big_in_array(_: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    let pair = |a| Value::Record(vec![Value::Int(a), Value::Str("x".into())]);
    Ok(Value::Array(vec![pair(1), pair(256)]))
}

fn array_for_record(_: &mut puy_core::ext::CallCtx<'_>) -> Result<Value, PasError> {
    Ok(Value::Array((0..100_000).map(Value::Int).collect()))
}

/// An extension whose functions break what their declarations say.
static BAD_EXT: Extension = Extension {
    unit: "BadExt",
    source: BAD_SOURCE,
    fingerprint: fingerprint(BAD_SOURCE),
    functions: &[
        Function::sync("Big", big),
        Function::sync("StrAsObj", str_as_obj),
        Function::sync("RaiseString", raise_string),
        Function::sync("CallWide", call_wide),
        Function::sync("BigInRecord", big_in_record),
        Function::sync("ShortRecord", short_record),
        Function::sync("NotArray", not_array),
        Function::sync("BigInArray", big_in_array),
        Function::sync("ArrayForRecord", array_for_record),
    ],
    classes: &[],
    capabilities: &[],
};

/// How a program using [`BAD_EXT`] ends: the message of its internal error (F0001).
fn fatal_of(body: &str) -> String {
    use puy_core::vm::RunOutcome;
    let src = format!("program P;\nuses BadExt;\nbegin\n  {body}\nend.\n");
    let opts = CompileOptions {
        extensions: ExtensionSet::new().with(&BAD_EXT),
        ..Default::default()
    };
    let mut provider = MemorySourceProvider::new();
    let out = compile("p.pas", src.into_bytes(), &mut provider, &opts);
    let m = out.stages.module.expect("compiled");
    let mut iso = isolate(m, ExtensionSet::new().with(&BAD_EXT)).expect("isolate");
    iso.start_main(vec!["p".into()]).unwrap();
    loop {
        match iso.run(10_000) {
            RunOutcome::Fatal(e) => return e.message,
            RunOutcome::Finished(st) => panic!("finished: {st:?}"),
            _ => {}
        }
    }
}

// What an extension gives that its declaration does not allow is an internal error (F0001;
// `docs/spec/extensions.md` 3, 5.3): an ordinal outside the declared type (a result, and an
// argument of a callback), a reference of another kind, and a raised value that is not an
// exception object.
#[test]
fn values_against_the_declaration_are_fatal() {
    assert_eq!(
        fatal_of("WriteLn(Big);"),
        "extension function `Big` gave 1000, outside the range 0..255 of its type"
    );
    assert_eq!(
        fatal_of("WriteLn(CallWide(function(B: Byte): Integer begin Result := B end));"),
        "extension function `a callback` gave 300, outside the range 0..255 of its type"
    );
    assert_eq!(
        fatal_of("WriteLn(StrAsObj('x' + ParamStr(0)) = nil);"),
        "extension function `StrAsObj` gave a value that is not of its declared type"
    );
    assert_eq!(
        fatal_of("RaiseString('x' + ParamStr(0));"),
        "an extension raised a value that is not an exception object"
    );
}

// The fields and elements of a record or an array an extension gives are checked as a result
// is (`docs/spec/extensions.md` 3): an ordinal outside its field's type, a record of another
// number of fields, a value of another kind than the array declared.
#[test]
fn records_and_arrays_against_the_declaration_are_fatal() {
    assert_eq!(
        fatal_of("WriteLn(BigInRecord.A);"),
        "extension function `BigInRecord` gave 1000, outside the range 0..255 of its type"
    );
    assert_eq!(
        fatal_of("WriteLn(ShortRecord.A);"),
        "extension function `ShortRecord` gave a record of 1 fields for one of 2"
    );
    assert_eq!(
        fatal_of("WriteLn(Length(NotArray));"),
        "extension function `NotArray` gave Int(1) for a value of kind `array`"
    );
    assert_eq!(
        fatal_of("WriteLn(Length(BigInArray));"),
        "extension function `BigInArray` gave 256, outside the range 0..255 of its type"
    );
    // A large value is cut short in the message
    assert_eq!(
        fatal_of("WriteLn(ArrayForRecord.A);"),
        "extension function `ArrayForRecord` gave Array([Int(0), Int(1), Int(2), Int(3), Int(4), \
         Int(5), Int(6… for a value of kind `record`"
    );
}

mod macros {
    include!("../../puy-ext-macros/src/reserved.rs");
}

// The attributes refuse the reserved words as generated names: their list is the lexer's table.
#[test]
fn macro_reserved_words_are_the_lexers() {
    let lexer: Vec<&str> = puy_core::syntax::SyntaxKind::reserved_words().collect();
    assert_eq!(macros::RESERVED_WORDS, lexer.as_slice());
}
