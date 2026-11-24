//! The Rust types a `native` routine can take and return, and the Pascal types they stand for
//! (requirements §6, the table of types; `docs/spec/extensions.md` 7.3).

use std::collections::HashMap;

use syn::spanned::Spanned;
use syn::{GenericArgument, PathArguments, ReturnType, Type};

/// What a Rust name of the module stands for.
#[derive(Clone)]
pub(crate) enum Known {
    /// A `#[pascal_class]` struct: the class's Pascal name.
    Class(String),
    /// A `#[pascal_enum]` enum.
    Enum(String),
    /// A `#[pascal_callback]` type.
    Callback(String),
    /// A `#[pascal_record]` struct.
    Record(String),
}

/// The names the module declares, by their Rust name.
pub(crate) type KnownTypes = HashMap<String, Known>;

/// How a Rust type is passed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Ty {
    /// An ordinal or a real, read with `FromArg` (an integer, `f32` / `f64`, `bool`, `char`,
    /// `PasChar`).
    Scalar(&'static str),
    /// An owned string read with `FromArg` (`String`, `PasString`, `Utf8String`).
    Text(&'static str),
    /// `&str`: borrowed from the argument (copied when the routine takes the `CallCtx` or is
    /// `async`).
    StrRef,
    /// `PasStr<'_>`: the string as the VM holds it.
    PasStr,
    /// `Vec<u8>`.
    Bytes,
    /// `&[u8]`: borrowed from the argument.
    BytesRef,
    /// `PasObj<C>` (or `Option<PasObj<C>>`): an instance held by Rust.
    Obj { class: String, opt: bool },
    /// `C` (or `Option<C>`) by value: a new instance (results only).
    NewObj { class: String, opt: bool },
    /// `PasFn<S>` (or `Option<PasFn<S>>`).
    Func { sig: String, opt: bool },
    /// A `#[pascal_enum]` enum.
    Enum(String),
    /// A `#[pascal_record]` struct, copied.
    Record(String),
    /// `Vec<T>` with `T` other than `u8`: a dynamic array, copied element by element.
    Array(Box<Ty>),
    /// `&[T]` with `T` other than `u8`: an argument copied into a `Vec<T>` it borrows.
    SliceRef(Box<Ty>),
    /// `&mut CallCtx<'_>`: the context, not a Pascal parameter.
    Ctx,
    /// `()`: no result.
    Unit,
}

impl Ty {
    /// The Pascal type (`None` for the context and for no result).
    pub(crate) fn pascal(&self) -> Option<String> {
        Some(match self {
            Ty::Scalar(p) | Ty::Text(p) => (*p).to_string(),
            Ty::StrRef | Ty::PasStr => "string".to_string(),
            Ty::Bytes | Ty::BytesRef => "TArray<Byte>".to_string(),
            Ty::Obj { class, .. } | Ty::NewObj { class, .. } => class.clone(),
            Ty::Func { sig, .. } => sig.clone(),
            Ty::Enum(e) | Ty::Record(e) => e.clone(),
            Ty::Array(t) | Ty::SliceRef(t) => format!("TArray<{}>", t.pascal()?),
            Ty::Ctx | Ty::Unit => return None,
        })
    }

    /// Whether a parameter of this type is declared `const` (a managed type) rather than by
    /// value.
    pub(crate) fn is_const_param(&self) -> bool {
        !matches!(self, Ty::Scalar(_) | Ty::Enum(_))
    }
}

/// The type `ty` as the parameter or result of a routine, in a method of the class `this` (whose
/// `Self` it may name).
pub(crate) fn classify(ty: &Type, known: &KnownTypes, this: Option<&str>) -> syn::Result<Ty> {
    let err = |msg: String| Err(syn::Error::new(ty.span(), msg));
    match ty {
        Type::Paren(p) => classify(&p.elem, known, this),
        Type::Group(g) => classify(&g.elem, known, this),
        Type::Tuple(t) if t.elems.is_empty() => Ok(Ty::Unit),
        Type::Reference(r) => {
            if r.mutability.is_some() {
                if last_ident(&r.elem).as_deref() == Some("CallCtx") {
                    return Ok(Ty::Ctx);
                }
                return err(
                    "a `native` routine cannot take a `&mut` reference (only `&mut CallCtx<'_>`)"
                        .to_string(),
                );
            }
            match &*r.elem {
                Type::Path(p) if p.path.is_ident("str") => Ok(Ty::StrRef),
                Type::Slice(s) if is_ident_type(&s.elem, "u8") => Ok(Ty::BytesRef),
                Type::Slice(s) => Ok(Ty::SliceRef(Box::new(element(&s.elem, known)?))),
                _ => err(
                    "a `native` routine takes references only as `&str` (`string`) and `&[T]` \
                     (`TArray<T>`); take `&self`, or an instance as `PasObj<T>`"
                        .to_string(),
                ),
            }
        }
        Type::Path(p) if p.qself.is_none() => {
            let seg = p.path.segments.last().expect("a path has a segment");
            let name = seg.ident.to_string();
            let scalar = |pascal| Ok(Ty::Scalar(pascal));
            match name.as_str() {
                "i8" => scalar("ShortInt"),
                "i16" => scalar("SmallInt"),
                "i32" => scalar("Integer"),
                "i64" => scalar("Int64"),
                "u8" => scalar("Byte"),
                "u16" => scalar("Word"),
                "u32" => scalar("Cardinal"),
                "u64" => scalar("UInt64"),
                "f32" => scalar("Single"),
                "f64" => scalar("Double"),
                "bool" => scalar("Boolean"),
                "char" => scalar("UCS4Char"),
                "PasChar" => scalar("Char"),
                "isize" | "usize" | "i128" | "u128" => err(format!(
                    "`{name}` has no Pascal counterpart: the sizes of Pascal's integers do not \
                     depend on the target (use `i32`, `i64`, …)"
                )),
                "String" | "PasString" => Ok(Ty::Text("string")),
                "Utf8String" => Ok(Ty::Text("UTF8String")),
                "PasStr" => Ok(Ty::PasStr),
                "Vec" => match type_arg(seg) {
                    Some(t) if is_ident_type(t, "u8") => Ok(Ty::Bytes),
                    Some(t) => Ok(Ty::Array(Box::new(element(t, known)?))),
                    None => err("`Vec` without a type".to_string()),
                },
                "PasObj" => match type_arg(seg).map(|t| class_of(t, known, this)) {
                    Some(Some(class)) => Ok(Ty::Obj { class, opt: false }),
                    _ => err(
                        "`PasObj<T>` takes a `#[pascal_class]` struct of the module (or `Self`)"
                            .to_string(),
                    ),
                },
                "PasFn" => match type_arg(seg).and_then(last_ident) {
                    Some(s) => match known.get(&s) {
                        Some(Known::Callback(sig)) => Ok(Ty::Func {
                            sig: sig.clone(),
                            opt: false,
                        }),
                        _ => err(format!(
                            "`{s}` is not a `#[pascal_callback]` type of the module"
                        )),
                    },
                    None => err(
                        "name the signature of the callable value: `PasFn<T>` with a \
                         `#[pascal_callback]` type `T` of the module"
                            .to_string(),
                    ),
                },
                "Option" => {
                    let Some(inner) = type_arg(seg) else {
                        return err("`Option` without a type".to_string());
                    };
                    match classify(inner, known, this)? {
                        Ty::Obj { class, opt: false } => Ok(Ty::Obj { class, opt: true }),
                        Ty::NewObj { class, opt: false } => Ok(Ty::NewObj { class, opt: true }),
                        Ty::Func { sig, opt: false } => Ok(Ty::Func { sig, opt: true }),
                        _ => err(
                            "`Option` is passed only around a class or a callable value (as \
                             `nil`); `Option` of a value type stands for `Nullable<T>`, which \
                             does not exist yet"
                                .to_string(),
                        ),
                    }
                }
                "Result" => err(
                    "`Result` is only the result of a routine (its `Err` raises an exception)"
                        .to_string(),
                ),
                _ => match class_of(ty, known, this) {
                    Some(class) => Ok(Ty::NewObj { class, opt: false }),
                    None => match known.get(&name) {
                        Some(Known::Enum(e)) => Ok(Ty::Enum(e.clone())),
                        Some(Known::Record(r)) => Ok(Ty::Record(r.clone())),
                        Some(Known::Callback(_)) => {
                            err(format!("a callable value is passed as `PasFn<{name}>`"))
                        }
                        _ => err(format!(
                            "type `{}` has no Pascal counterpart (requirements §6, the table of \
                             types)",
                            quote::ToTokens::to_token_stream(ty)
                        )),
                    },
                },
            }
        }
        _ => err(format!(
            "type `{}` has no Pascal counterpart (requirements §6, the table of types)",
            quote::ToTokens::to_token_stream(ty)
        )),
    }
}

/// The result of a routine: its type, and whether it is a `Result` whose `Err` raises.
/// The `Output` of a return type `impl Future<Output = T>` (with other bounds such as `'static`
/// allowed): the type a routine that starts its work in the call returns
/// (`docs/spec/extensions.md` 7.5).
pub(crate) fn future_output(ret: &ReturnType) -> Option<ReturnType> {
    let ReturnType::Type(arrow, ty) = ret else {
        return None;
    };
    let Type::ImplTrait(it) = &**ty else {
        return None;
    };
    it.bounds.iter().find_map(|b| {
        let syn::TypeParamBound::Trait(t) = b else {
            return None;
        };
        let seg = t.path.segments.last()?;
        if seg.ident != "Future" {
            return None;
        }
        let PathArguments::AngleBracketed(a) = &seg.arguments else {
            return None;
        };
        a.args.iter().find_map(|g| match g {
            GenericArgument::AssocType(at) if at.ident == "Output" => {
                Some(ReturnType::Type(*arrow, Box::new(at.ty.clone())))
            }
            _ => None,
        })
    })
}

pub(crate) fn classify_return(
    ret: &ReturnType,
    known: &KnownTypes,
    this: Option<&str>,
) -> syn::Result<(Ty, bool)> {
    let ty = match ret {
        ReturnType::Default => return Ok((Ty::Unit, false)),
        ReturnType::Type(_, ty) => &**ty,
    };
    let (inner, is_result) = match ty {
        Type::Path(p) if p.qself.is_none() => {
            let seg = p.path.segments.last().expect("a path has a segment");
            if seg.ident == "Result" {
                match type_arg(seg) {
                    Some(t) => (t, true),
                    None => return Err(syn::Error::new(ty.span(), "`Result` without a type")),
                }
            } else {
                (ty, false)
            }
        }
        _ => (ty, false),
    };
    let t = classify(inner, known, this)?;
    match t {
        Ty::StrRef | Ty::PasStr | Ty::BytesRef | Ty::SliceRef(_) | Ty::Ctx => Err(syn::Error::new(
            inner.span(),
            "a `native` routine returns owned values (`String`, `PasString`, `Vec<u8>`, …), not \
             references",
        )),
        t => Ok((t, is_result)),
    }
}

/// The type of a field of a record or an element of a dynamic array: an ordinal, a real, an
/// owned string, a `Vec`, an enumeration or a record (`docs/spec/extensions.md` 3: copied with
/// the value that holds it).
pub(crate) fn element(ty: &Type, known: &KnownTypes) -> syn::Result<Ty> {
    let t = classify(ty, known, None)?;
    if is_element(&t) {
        return Ok(t);
    }
    Err(syn::Error::new(
        ty.span(),
        "a field of a record or an element of a dynamic array is an ordinal, a real, an owned \
         string (`String`, `PasString`, `Utf8String`), a `Vec`, a `#[pascal_enum]` or a \
         `#[pascal_record]` (`docs/spec/extensions.md` 3)",
    ))
}

/// Whether a value of this type can be a field of a record or an element of a dynamic array.
pub(crate) fn is_element(t: &Ty) -> bool {
    match t {
        Ty::Scalar(_) | Ty::Text(_) | Ty::Bytes | Ty::Enum(_) | Ty::Record(_) => true,
        Ty::Array(t) => is_element(t),
        _ => false,
    }
}

/// The Pascal class a type names: `Self` in a method, or a `#[pascal_class]` struct.
fn class_of(ty: &Type, known: &KnownTypes, this: Option<&str>) -> Option<String> {
    let name = last_ident(ty)?;
    if name == "Self" {
        return this.map(str::to_string);
    }
    match known.get(&name) {
        Some(Known::Class(c)) => Some(c.clone()),
        _ => None,
    }
}

/// The last name of a path type without type arguments of its own (`Counter`, `crate::Counter`).
pub(crate) fn last_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) if p.qself.is_none() => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn is_ident_type(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident(name))
}

/// The first type argument of a path segment (`T` of `Vec<T>`).
fn type_arg(seg: &syn::PathSegment) -> Option<&Type> {
    match &seg.arguments {
        PathArguments::AngleBracketed(a) => a.args.iter().find_map(|g| match g {
            GenericArgument::Type(t) => Some(t),
            _ => None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> KnownTypes {
        let mut k = KnownTypes::new();
        k.insert("Counter".into(), Known::Class("TCounter".into()));
        k.insert("Color".into(), Known::Enum("TColor".into()));
        k.insert("IntFunc".into(), Known::Callback("TIntFunc".into()));
        k.insert("Point".into(), Known::Record("TPoint".into()));
        k
    }

    fn pascal(src: &str) -> String {
        let ty: Type = syn::parse_str(src).expect("a type");
        classify(&ty, &known(), Some("TCounter"))
            .map(|t| t.pascal().unwrap_or_default())
            .unwrap_or_else(|e| format!("error: {e}"))
    }

    // The table of requirements §6, as the generated declarations write it.
    #[test]
    fn maps_rust_types_to_pascal() {
        for (rust, p) in [
            ("i8", "ShortInt"),
            ("i32", "Integer"),
            ("u64", "UInt64"),
            ("f32", "Single"),
            ("bool", "Boolean"),
            ("char", "UCS4Char"),
            ("PasChar", "Char"),
            ("&str", "string"),
            ("String", "string"),
            ("PasStr<'_>", "string"),
            ("Utf8String", "UTF8String"),
            ("&[u8]", "TArray<Byte>"),
            ("Vec<u8>", "TArray<Byte>"),
            ("PasObj<Self>", "TCounter"),
            ("Option<PasObj<Counter>>", "TCounter"),
            ("Counter", "TCounter"),
            ("PasFn<IntFunc>", "TIntFunc"),
            ("Color", "TColor"),
            ("Point", "TPoint"),
            ("Vec<i32>", "TArray<Integer>"),
            ("&[f64]", "TArray<Double>"),
            ("Vec<Point>", "TArray<TPoint>"),
            ("Vec<Vec<String>>", "TArray<TArray<string>>"),
            ("Vec<Vec<u8>>", "TArray<TArray<Byte>>"),
            ("&[Color]", "TArray<TColor>"),
        ] {
            assert_eq!(pascal(rust), p, "{rust}");
        }
    }

    // Types without a counterpart are errors that name the way out.
    #[test]
    fn refuses_types_without_a_counterpart() {
        for (rust, msg) in [
            ("usize", "`usize` has no Pascal counterpart"),
            (
                "Vec<&str>",
                "a field of a record or an element of a dynamic array",
            ),
            ("Vec<PasObj<Counter>>", "a field of a record or an element"),
            ("&[&[u8]]", "a field of a record or an element"),
            ("Option<i32>", "`Nullable<T>`"),
            ("PasFn", "name the signature"),
            ("&mut [u8]", "`&mut` reference"),
            ("&i32", "takes references only"),
            (
                "HashMap<i32, i32>",
                "type `HashMap < i32 , i32 >` has no Pascal counterpart",
            ),
            ("IntFunc", "`PasFn<IntFunc>`"),
        ] {
            let got = pascal(rust);
            assert!(got.contains(msg), "{rust}: {got}");
        }
    }
}
