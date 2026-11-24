//! `#[pascal_module]`: reads the items of the module that carry the attributes, and generates
//! the unit's Pascal declarations, the calls of the Rust functions and the registration table
//! (`docs/design/extensions.md` 3.11, `docs/spec/extensions.md` chapter 7).

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{
    Attribute, Expr, FnArg, ImplItem, Item, ItemConst, ItemEnum, ItemImpl, ItemMod, ItemStruct,
    ItemType, Lit, LitStr, Pat, ReceiverKind, Signature, Type, UnOp,
};

use crate::names::{Scope, check_ident, check_unit_name, pascal_case};
use crate::types::{
    Known, KnownTypes, Ty, classify, classify_return, element, future_output, last_ident,
};

/// The attributes `#[pascal_module]` reads inside the module.
const ITEM_ATTRS: &[&str] = &[
    "pascal_function",
    "pascal_class",
    "pascal_methods",
    "pascal_constructor",
    "pascal_exception",
    "pascal_enum",
    "pascal_const",
    "pascal_callback",
    "pascal_record",
];

pub(crate) fn module(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let mut m: ItemMod = syn::parse2(item)?;
    let (tokens, _) = expand(attr, &mut m)?;
    Ok(tokens)
}

/// Expands the module `m` in place, and returns it with the generated items, and the unit's
/// source.
fn expand(attr: TokenStream, m: &mut ItemMod) -> syn::Result<(TokenStream, String)> {
    let args = ModuleArgs::parse(attr)?;
    let Some((_, items)) = m.content.as_mut() else {
        return Err(syn::Error::new(
            m.span(),
            "`#[pascal_module]` needs the module's items inline (`mod name { … }`)",
        ));
    };
    let mut unit = Unit::default();
    // First the types, so that the routines can name them
    for item in items.iter_mut() {
        unit.read_type(item)?;
    }
    unit.check_exception_parents()?;
    for item in items.iter_mut() {
        unit.read_routines(item)?;
    }
    let source = unit.source(&args.unit);
    let generated = unit.generate(&args, &source);
    items.extend(generated);
    Ok((quote!(#m), source))
}

/// The arguments of `#[pascal_module]`: `unit = "Puy.Zip"`, `capabilities(fs, net)`.
struct ModuleArgs {
    unit: String,
    capabilities: Vec<String>,
}

impl ModuleArgs {
    fn parse(attr: TokenStream) -> syn::Result<ModuleArgs> {
        let mut unit = None;
        let mut capabilities = Vec::new();
        let parser = syn::meta::parser(|meta| {
            if meta.path.is_ident("unit") {
                let s: LitStr = meta.value()?.parse()?;
                check_unit_name(&s.value(), s.span())?;
                unit = Some(s.value());
                Ok(())
            } else if meta.path.is_ident("capabilities") {
                meta.parse_nested_meta(|cap| {
                    let name = cap
                        .path
                        .get_ident()
                        .ok_or_else(|| cap.error("a capability is a name (`fs`, `net`, …)"))?;
                    capabilities.push(name.to_string());
                    Ok(())
                })
            } else {
                Err(meta.error("expected `unit = \"…\"` or `capabilities(…)`"))
            }
        });
        syn::parse::Parser::parse2(parser, attr)?;
        let unit = unit.ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "`#[pascal_module]` needs the unit's name: `#[pascal_module(unit = \"…\")]`",
            )
        })?;
        Ok(ModuleArgs { unit, capabilities })
    }
}

/// The arguments of an attribute on an item: `name = "…"` and the ones its kind takes.
#[derive(Default)]
struct ItemArgs {
    name: Option<LitStr>,
    parent: Option<String>,
    cancel: Option<LitStr>,
    getter: Option<LitStr>,
    setter: Option<LitStr>,
}

impl ItemArgs {
    /// Parses the arguments of `attr`, which may be the keys `keys` besides `name`.
    fn parse(attr: &Attribute, keys: &[&str]) -> syn::Result<ItemArgs> {
        let mut args = ItemArgs::default();
        if matches!(attr.meta, syn::Meta::Path(_)) {
            return Ok(args);
        }
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            if key != "name" && !keys.contains(&key.as_str()) {
                let mut expected = vec!["name".to_string()];
                expected.extend(keys.iter().map(|k| k.to_string()));
                return Err(meta.error(format!(
                    "unknown argument; expected {}",
                    expected
                        .iter()
                        .map(|k| format!("`{k}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            let value = meta.value()?;
            if key == "parent" {
                // `parent = EArgumentException` or `parent = "EArgumentException"`
                let parent = if value.peek(LitStr) {
                    value.parse::<LitStr>()?.value()
                } else {
                    value.parse::<syn::Ident>()?.to_string()
                };
                args.parent = Some(parent);
                return Ok(());
            }
            let s: LitStr = value.parse()?;
            match key.as_str() {
                "name" => args.name = Some(s),
                "cancel" => args.cancel = Some(s),
                "getter" => args.getter = Some(s),
                _ => args.setter = Some(s),
            }
            Ok(())
        })?;
        Ok(args)
    }

    /// The Pascal name: the one given, or `default`.
    fn name_or(&self, default: impl FnOnce() -> String, span: Span) -> syn::Result<String> {
        let (name, span) = match &self.name {
            Some(n) => (n.value(), n.span()),
            None => (default(), span),
        };
        check_ident(&name, span)?;
        Ok(name)
    }
}

/// Whether `attr` is the attribute `name` (`#[name]` or `#[puy_ext::name]`).
fn is_attr(attr: &Attribute, name: &str) -> bool {
    attr.path().segments.last().is_some_and(|s| s.ident == name)
}

/// Removes the attribute `name` from `attrs` and returns it.
fn take_attr(attrs: &mut Vec<Attribute>, name: &str) -> syn::Result<Option<Attribute>> {
    let mut found: Option<Attribute> = None;
    let mut rest = Vec::new();
    for a in attrs.drain(..) {
        if is_attr(&a, name) {
            if found.is_some() {
                return Err(syn::Error::new(
                    a.span(),
                    format!("`#[{name}]` is written twice"),
                ));
            }
            found = Some(a);
        } else {
            rest.push(a);
        }
    }
    *attrs = rest;
    Ok(found)
}

/// Refuses an attribute of ours that is not read where it is written.
fn refuse_attrs(attrs: &[Attribute], what: &str) -> syn::Result<()> {
    for a in attrs {
        if let Some(name) = ITEM_ATTRS.iter().find(|n| is_attr(a, n)) {
            return Err(syn::Error::new(
                a.span(),
                format!("`#[{name}]` cannot be written on {what}"),
            ));
        }
    }
    Ok(())
}

/// The lines of the `///` comments of an item, to write before its Pascal declaration
/// (requirements §6).
fn doc_lines(attrs: &[Attribute]) -> Vec<String> {
    let mut lines = Vec::new();
    for a in attrs {
        if !a.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &a.meta
            && let Expr::Lit(l) = &nv.value
            && let Lit::Str(s) = &l.lit
        {
            for line in s.value().lines() {
                let line = line.strip_prefix(' ').unwrap_or(line);
                lines.push(line.trim_end().to_string());
            }
        }
    }
    lines
}

/// Writes the `///` lines `doc` with the indentation `indent`.
fn write_doc(out: &mut String, doc: &[String], indent: &str) {
    for line in doc {
        if line.is_empty() {
            out.push_str(&format!("{indent}///\n"));
        } else {
            out.push_str(&format!("{indent}/// {line}\n"));
        }
    }
}

struct Const {
    doc: Vec<String>,
    name: String,
    value: String,
}

struct Exception {
    doc: Vec<String>,
    name: String,
    parent: String,
    rust: syn::Ident,
    span: Span,
}

struct Enum {
    doc: Vec<String>,
    name: String,
    rust: syn::Ident,
    /// The values: their Rust and Pascal names.
    values: Vec<(syn::Ident, String)>,
}

struct Record {
    doc: Vec<String>,
    name: String,
    rust: syn::Ident,
    /// The fields: their documentation, Rust name, Pascal name and Pascal type.
    fields: Vec<(Vec<String>, syn::Ident, String, String)>,
}

struct Callback {
    doc: Vec<String>,
    name: String,
    params: String,
    ret: Option<String>,
}

struct Class {
    doc: Vec<String>,
    name: String,
    rust: syn::Ident,
    members: Vec<(Vec<String>, String)>,
    scope: Scope,
    properties: Vec<Property>,
}

/// A property of a class, from the methods marked `getter` / `setter`.
struct Property {
    name: String,
    ty: String,
    getter: Option<String>,
    setter: Option<String>,
}

/// A `native` routine: its registration and the function that calls the Rust one.
struct Routine {
    /// The name in the registration table (`Crc32`, `TZipArchive.EntryCount`).
    name: String,
    wrapper: syn::Ident,
    is_async: bool,
    cancel: TokenStream,
}

#[derive(Default)]
struct Unit {
    known: KnownTypes,
    /// The names of the unit's scope.
    scope: Scope,
    consts: Vec<Const>,
    exceptions: Vec<Exception>,
    enums: Vec<Enum>,
    records: Vec<Record>,
    callbacks: Vec<Callback>,
    classes: Vec<Class>,
    /// The declarations of the routines outside classes.
    routines: Vec<(Vec<String>, String)>,
    registered: Vec<Routine>,
    wrappers: Vec<TokenStream>,
}

/// How a method receives its instance.
#[derive(Clone, Copy, PartialEq)]
enum This {
    /// A routine outside a class, a class method or a constructor.
    None,
    /// `&self`.
    Ref,
    /// `&mut self`.
    RefMut,
    /// A first parameter `PasObj<Self>`.
    Obj,
}

/// A parameter of a routine.
struct Param {
    ty: Ty,
    /// The Pascal name (`None` for the context and for the instance).
    pascal: Option<String>,
}

impl Unit {
    /// Reads a type, a constant or an exception declared with an attribute.
    fn read_type(&mut self, item: &mut Item) -> syn::Result<()> {
        match item {
            Item::Struct(s) => self.read_struct(s),
            Item::Enum(e) => {
                let Some(attr) = take_attr(&mut e.attrs, "pascal_enum")? else {
                    return refuse_attrs(&e.attrs, "an enum (only `#[pascal_enum]`)");
                };
                self.read_enum(e, &attr)
            }
            Item::Type(t) => {
                let Some(attr) = take_attr(&mut t.attrs, "pascal_callback")? else {
                    return refuse_attrs(&t.attrs, "a type alias (only `#[pascal_callback]`)");
                };
                self.read_callback(t, &attr)
            }
            Item::Const(c) => {
                let Some(attr) = take_attr(&mut c.attrs, "pascal_const")? else {
                    return refuse_attrs(&c.attrs, "a constant (only `#[pascal_const]`)");
                };
                self.read_const(c, &attr)
            }
            _ => Ok(()),
        }
    }

    fn read_struct(&mut self, s: &mut ItemStruct) -> syn::Result<()> {
        if let Some(a) = take_attr(&mut s.attrs, "pascal_record")? {
            refuse_attrs(&s.attrs, "a struct (only one of the attributes)")?;
            return self.read_record(s, &a);
        }
        let class = take_attr(&mut s.attrs, "pascal_class")?;
        let exception = take_attr(&mut s.attrs, "pascal_exception")?;
        refuse_attrs(
            &s.attrs,
            "a struct (only `#[pascal_class]` and `#[pascal_exception]`)",
        )?;
        let doc = doc_lines(&s.attrs);
        match (class, exception) {
            (Some(a), None) => {
                no_generics(&s.generics, "a class")?;
                let args = ItemArgs::parse(&a, &[])?;
                let Some(name) = &args.name else {
                    return Err(syn::Error::new(
                        a.span(),
                        "a class is named in Pascal: `#[pascal_class(name = \"T…\")]`",
                    ));
                };
                let name = name.value();
                check_ident(&name, a.span())?;
                self.scope.declare(&name, "the name", a.span())?;
                self.known
                    .insert(s.ident.to_string(), Known::Class(name.clone()));
                self.classes.push(Class {
                    doc,
                    name,
                    rust: s.ident.clone(),
                    members: Vec::new(),
                    scope: Scope::default(),
                    properties: Vec::new(),
                });
                Ok(())
            }
            (None, Some(a)) => {
                no_generics(&s.generics, "an exception")?;
                if !matches!(s.fields, syn::Fields::Unit) {
                    return Err(syn::Error::new(
                        s.fields.span(),
                        "an exception's type is a marker without fields (`pub struct EFoo;`): the \
                         exception's data is its message",
                    ));
                }
                let args = ItemArgs::parse(&a, &["parent"])?;
                let Some(name) = &args.name else {
                    return Err(syn::Error::new(
                        a.span(),
                        "an exception is named in Pascal: `#[pascal_exception(name = \"E…\")]`",
                    ));
                };
                let name = name.value();
                check_ident(&name, a.span())?;
                let parent = args.parent.unwrap_or_else(|| "Exception".to_string());
                check_ident(&parent, a.span())?;
                self.scope.declare(&name, "the name", a.span())?;
                // Never constructed: the marker only names the class
                s.attrs.push(syn::parse_quote!(#[allow(dead_code)]));
                self.exceptions.push(Exception {
                    doc,
                    name,
                    parent,
                    rust: s.ident.clone(),
                    span: a.span(),
                });
                Ok(())
            }
            (Some(a), Some(_)) => Err(syn::Error::new(
                a.span(),
                "a struct is either a class or an exception",
            )),
            (None, None) => Ok(()),
        }
    }

    fn read_record(&mut self, s: &mut ItemStruct, attr: &Attribute) -> syn::Result<()> {
        no_generics(&s.generics, "a record")?;
        let args = ItemArgs::parse(attr, &[])?;
        let Some(name) = &args.name else {
            return Err(syn::Error::new(
                attr.span(),
                "a record is named in Pascal: `#[pascal_record(name = \"T…\")]`",
            ));
        };
        let name = name.value();
        check_ident(&name, attr.span())?;
        let syn::Fields::Named(named) = &mut s.fields else {
            return Err(syn::Error::new(
                s.fields.span(),
                "a record's fields are named (`pub struct TPoint { x: i32, y: i32 }`)",
            ));
        };
        // The fields are written qualified by the record's value: their names are of the
        // record's own scope
        let mut scope = Scope::default();
        let mut fields = Vec::new();
        for f in named.named.iter_mut() {
            let ident = f.ident.clone().expect("a named field");
            let fname = match take_attr(&mut f.attrs, "pascal")? {
                Some(a) => ItemArgs::parse(&a, &[])?
                    .name_or(|| pascal_case(&ident.to_string()), f.span())?,
                None => pascal_case(strip_raw(&ident.to_string())),
            };
            check_ident(&fname, f.span())?;
            scope.declare(&fname, "the field", f.span())?;
            // A record of the module is named once it is read: a field names an earlier one
            let ty = element(&f.ty, &self.known)?;
            fields.push((
                doc_lines(&f.attrs),
                ident,
                fname,
                ty.pascal().expect("a field has a Pascal type"),
            ));
        }
        self.scope.declare(&name, "the name", attr.span())?;
        self.known
            .insert(s.ident.to_string(), Known::Record(name.clone()));
        self.records.push(Record {
            doc: doc_lines(&s.attrs),
            name,
            rust: s.ident.clone(),
            fields,
        });
        Ok(())
    }

    fn read_enum(&mut self, e: &mut ItemEnum, attr: &Attribute) -> syn::Result<()> {
        no_generics(&e.generics, "an enumeration")?;
        let args = ItemArgs::parse(attr, &[])?;
        let Some(name) = &args.name else {
            return Err(syn::Error::new(
                attr.span(),
                "an enumeration is named in Pascal: `#[pascal_enum(name = \"T…\")]`",
            ));
        };
        let name = name.value();
        check_ident(&name, attr.span())?;
        self.scope.declare(&name, "the name", attr.span())?;
        let mut values = Vec::new();
        // The values are written qualified (`TColor.Red`; `docs/spec/types.md` 8.1): their names
        // are of the enumeration's own scope
        let mut scope = Scope::default();
        for v in e.variants.iter_mut() {
            if !matches!(v.fields, syn::Fields::Unit) || v.discriminant.is_some() {
                return Err(syn::Error::new(
                    v.span(),
                    "the values of a `#[pascal_enum]` have neither fields nor discriminants",
                ));
            }
            let vname = match take_attr(&mut v.attrs, "pascal")? {
                Some(a) => ItemArgs::parse(&a, &[])?.name_or(|| v.ident.to_string(), v.span())?,
                None => {
                    let n = v.ident.to_string();
                    check_ident(&n, v.span())?;
                    n
                }
            };
            scope.declare(&vname, "the value", v.span())?;
            values.push((v.ident.clone(), vname));
        }
        if values.is_empty() {
            return Err(syn::Error::new(
                e.span(),
                "an enumeration has at least one value",
            ));
        }
        self.known
            .insert(e.ident.to_string(), Known::Enum(name.clone()));
        self.enums.push(Enum {
            doc: doc_lines(&e.attrs),
            name,
            rust: e.ident.clone(),
            values,
        });
        Ok(())
    }

    fn read_callback(&mut self, t: &ItemType, attr: &Attribute) -> syn::Result<()> {
        no_generics(&t.generics, "a callback type")?;
        let args = ItemArgs::parse(attr, &[])?;
        let Some(name) = &args.name else {
            return Err(syn::Error::new(
                attr.span(),
                "a callback type is named in Pascal: `#[pascal_callback(name = \"T…\")]`",
            ));
        };
        let name = name.value();
        check_ident(&name, attr.span())?;
        let Type::FnPtr(f) = &*t.ty else {
            return Err(syn::Error::new(
                t.ty.span(),
                "a `#[pascal_callback]` type is a function type: `fn(x: i32) -> i32`",
            ));
        };
        self.scope.declare(&name, "the name", attr.span())?;
        // Known before its signature is read, which cannot name it (a callable value is not an
        // argument of a callable value)
        self.known
            .insert(t.ident.to_string(), Known::Callback(name.clone()));
        let mut scope = Scope::default();
        let mut params = Vec::new();
        for (i, arg) in f.inputs.iter().enumerate() {
            let ty = classify(&arg.ty, &self.known, None)?;
            if !callback_type(&ty) || matches!(ty, Ty::Unit) {
                return Err(syn::Error::new(
                    arg.ty.span(),
                    "a callable value's parameters are ordinals, reals, strings and instances \
                     (`docs/spec/extensions.md` 3)",
                ));
            }
            let pname = match &arg.name {
                Some((id, _)) => pascal_case(&id.to_string()),
                None => format!("Arg{}", i + 1),
            };
            param_name(&pname, arg.span(), &mut scope)?;
            params.push(param_text(&pname, &ty));
        }
        let (ret, _) = classify_return(&f.output, &self.known, None)?;
        if !callback_type(&ret) {
            return Err(syn::Error::new(
                f.output.span(),
                "a callable value returns an ordinal, a real, a string or an instance \
                 (`docs/spec/extensions.md` 3)",
            ));
        }
        self.callbacks.push(Callback {
            doc: doc_lines(&t.attrs),
            name,
            params: params.join("; "),
            ret: ret.pascal(),
        });
        Ok(())
    }

    fn read_const(&mut self, c: &mut ItemConst, attr: &Attribute) -> syn::Result<()> {
        let args = ItemArgs::parse(attr, &[])?;
        let name = args.name_or(|| pascal_case(&c.ident.to_string()), attr.span())?;
        self.scope.declare(&name, "the name", attr.span())?;
        let value = pascal_literal(&c.expr)?;
        self.consts.push(Const {
            doc: doc_lines(&c.attrs),
            name,
            value,
        });
        // Used by Pascal, which the Rust compiler does not see
        c.attrs.push(syn::parse_quote!(#[allow(dead_code)]));
        Ok(())
    }

    /// Checks that an exception whose parent is an exception of the module comes after it: the
    /// unit declares the exceptions in the order of the Rust items.
    fn check_exception_parents(&self) -> syn::Result<()> {
        for (i, e) in self.exceptions.iter().enumerate() {
            if let Some(j) = self
                .exceptions
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(&e.parent))
                && j >= i
            {
                let msg = if i == j {
                    format!("the exception `{}` is its own parent", e.name)
                } else {
                    format!(
                        "the parent `{}` of `{}` is declared after it; write the parent's item \
                         first (the unit declares the exceptions in the order of the items)",
                        e.parent, e.name
                    )
                };
                return Err(syn::Error::new(e.span, msg));
            }
        }
        Ok(())
    }

    /// Reads a routine, or the methods of a class.
    fn read_routines(&mut self, item: &mut Item) -> syn::Result<()> {
        match item {
            Item::Fn(f) => {
                let Some(attr) = take_attr(&mut f.attrs, "pascal_function")? else {
                    return refuse_attrs(&f.attrs, "a function (only `#[pascal_function]`)");
                };
                refuse_attrs(&f.attrs, "a function (only `#[pascal_function]`)")?;
                let args = ItemArgs::parse(&attr, &["cancel"])?;
                let doc = doc_lines(&f.attrs);
                let decl = self.routine(&f.sig, &args, None, false, attr.span())?;
                self.routines.push((doc, decl));
                Ok(())
            }
            Item::Impl(i) => {
                let Some(_) = take_attr(&mut i.attrs, "pascal_methods")? else {
                    refuse_attrs(&i.attrs, "an impl block (only `#[pascal_methods]`)")?;
                    for it in &i.items {
                        if let ImplItem::Fn(f) = it {
                            refuse_attrs(
                                &f.attrs,
                                "a method outside `#[pascal_methods]` impl blocks",
                            )?;
                        }
                    }
                    return Ok(());
                };
                refuse_attrs(&i.attrs, "an impl block (only `#[pascal_methods]`)")?;
                self.read_methods(i)
            }
            Item::Struct(_) | Item::Enum(_) | Item::Type(_) | Item::Const(_) => Ok(()),
            Item::Mod(m) => refuse_attrs(&m.attrs, "a nested module"),
            Item::Static(s) => refuse_attrs(&s.attrs, "a static"),
            Item::Trait(t) => refuse_attrs(&t.attrs, "a trait"),
            _ => Ok(()),
        }
    }

    fn read_methods(&mut self, i: &mut ItemImpl) -> syn::Result<()> {
        if i.trait_.is_some() {
            return Err(syn::Error::new(
                i.span(),
                "`#[pascal_methods]` is written on an inherent impl block (`impl Foo { … }`)",
            ));
        }
        no_generics(&i.generics, "a class")?;
        let rust = last_ident(&i.self_ty).unwrap_or_default();
        let Some(ci) = self.classes.iter().position(|c| c.rust == rust) else {
            return Err(syn::Error::new(
                i.self_ty.span(),
                "`#[pascal_methods]` is written on the methods of a `#[pascal_class]` struct of \
                 the module",
            ));
        };
        let class = (self.classes[ci].rust.clone(), self.classes[ci].name.clone());
        for it in i.items.iter_mut() {
            let ImplItem::Fn(f) = it else {
                continue;
            };
            let ctor = take_attr(&mut f.attrs, "pascal_constructor")?;
            let func = take_attr(&mut f.attrs, "pascal_function")?;
            refuse_attrs(
                &f.attrs,
                "a method (only `#[pascal_constructor]` and `#[pascal_function]`)",
            )?;
            let public = matches!(f.vis, syn::Visibility::Public(_));
            let (args, is_ctor, span) = match (ctor, func) {
                (Some(a), None) => (ItemArgs::parse(&a, &[])?, true, a.span()),
                (None, Some(a)) => (
                    ItemArgs::parse(&a, &["cancel", "getter", "setter"])?,
                    false,
                    a.span(),
                ),
                (Some(a), Some(_)) => {
                    return Err(syn::Error::new(
                        a.span(),
                        "a constructor takes its arguments in `#[pascal_constructor(…)]`",
                    ));
                }
                // A public method without attributes is exported; a private one is a helper
                (None, None) if public => (ItemArgs::default(), false, f.sig.ident.span()),
                (None, None) => continue,
            };
            let doc = doc_lines(&f.attrs);
            let decl = self.routine(&f.sig, &args, Some(&class), is_ctor, span)?;
            self.classes[ci].members.push((doc, decl));
        }
        Ok(())
    }

    /// Reads a routine's signature: writes its call, registers it and returns its Pascal
    /// declaration (without `///` lines and indentation).
    fn routine(
        &mut self,
        sig: &Signature,
        args: &ItemArgs,
        class: Option<&(syn::Ident, String)>,
        is_ctor: bool,
        span: Span,
    ) -> syn::Result<String> {
        no_generics(&sig.generics, "a `native` routine")?;
        if sig.variadic.is_some() || sig.abi.is_some() {
            return Err(syn::Error::new(
                sig.span(),
                "a `native` routine is a plain Rust function",
            ));
        }
        let is_async = sig.asyncness.is_some();
        // A plain `fn` returning `impl Future`: it runs in the call (it may take the `CallCtx`
        // and start host calls) and gives back the work to wait for
        let started = if is_async {
            None
        } else {
            future_output(&sig.output)
        };
        let suspends = is_async || started.is_some();
        let rust_name = sig.ident.to_string();
        let default_name = || {
            if is_ctor && rust_name == "new" {
                "Create".to_string()
            } else {
                pascal_case(&rust_name)
            }
        };
        let name = args.name_or(default_name, sig.ident.span())?;
        let this_class = class.map(|c| c.1.as_str());

        // The instance
        let mut this = This::None;
        let mut params = Vec::new();
        for (k, input) in sig.inputs.iter().enumerate() {
            match input {
                FnArg::Receiver(r) => {
                    this = match &r.kind {
                        ReceiverKind::Reference(_, _, None) => This::Ref,
                        ReceiverKind::Reference(_, _, Some(_)) => This::RefMut,
                        _ => {
                            return Err(syn::Error::new(
                                r.span(),
                                "a method takes its instance as `&self`, `&mut self` or \
                                 `this: PasObj<Self>`",
                            ));
                        }
                    };
                }
                FnArg::Typed(pt) => {
                    let Pat::Ident(id) = &*pt.pat else {
                        return Err(syn::Error::new(
                            pt.pat.span(),
                            "a parameter of a `native` routine is a name",
                        ));
                    };
                    let ty = classify(&pt.ty, &self.known, this_class)?;
                    if ty == Ty::Unit {
                        return Err(syn::Error::new(pt.ty.span(), "a parameter of type `()`"));
                    }
                    if let Ty::NewObj { .. } = ty {
                        return Err(syn::Error::new(
                            pt.ty.span(),
                            "an instance is taken as `PasObj<T>` (a class's value is not moved \
                             out of its instance)",
                        ));
                    }
                    let is_this = k == 0
                        && class.is_some()
                        && !is_ctor
                        && matches!(&*pt.ty, Type::Path(p)
                            if p.path.segments.last().is_some_and(|s| s.ident == "PasObj")
                                && quote!(#p).to_string().replace(' ', "").ends_with("<Self>"));
                    if is_this {
                        this = This::Obj;
                        params.push(Param { ty, pascal: None });
                    } else if ty == Ty::Ctx {
                        params.push(Param { ty, pascal: None });
                    } else {
                        params.push(Param {
                            ty,
                            pascal: Some(pascal_case(&id.ident.to_string())),
                        });
                    }
                }
            }
        }
        if this != This::None && class.is_none() {
            return Err(syn::Error::new(sig.span(), "`self` outside a class"));
        }
        if is_ctor && this != This::None {
            return Err(syn::Error::new(
                sig.span(),
                "a constructor makes the instance: it takes no `self`",
            ));
        }
        let takes_ctx = params.iter().any(|p| p.ty == Ty::Ctx);
        if is_async {
            if matches!(this, This::Ref | This::RefMut) {
                return Err(syn::Error::new(
                    sig.span(),
                    "an `async` method cannot borrow `self` across `await`: take \
                     `this: PasObj<Self>` and borrow it between the awaits (requirements §6)",
                ));
            }
            if takes_ctx {
                return Err(syn::Error::new(
                    sig.span(),
                    "an `async` routine cannot take the `CallCtx`: its work outlives the call",
                ));
            }
        }
        if is_ctor && suspends {
            return Err(syn::Error::new(sig.span(), "a constructor is not `async`"));
        }
        let cancel = match &args.cancel {
            None => quote!(::puy_ext::CancelMode::Drop),
            Some(c) if !suspends => {
                return Err(syn::Error::new(
                    c.span(),
                    "`cancel` is declared for an `async` routine only",
                ));
            }
            Some(c) => match c.value().as_str() {
                "drop" => quote!(::puy_ext::CancelMode::Drop),
                "detach" => quote!(::puy_ext::CancelMode::Detach),
                "complete" => quote!(::puy_ext::CancelMode::Complete),
                _ => {
                    return Err(syn::Error::new(
                        c.span(),
                        "`cancel` is \"drop\", \"detach\" or \"complete\"",
                    ));
                }
            },
        };
        let output = started.clone().unwrap_or_else(|| sig.output.clone());
        let (ret, is_result) = classify_return(&output, &self.known, this_class)?;
        if is_ctor
            && !matches!(&ret, Ty::NewObj { class: c, opt: false } if Some(c.as_str()) == this_class)
        {
            return Err(syn::Error::new(
                sig.output.span(),
                "a constructor returns `Self` (or `Result<Self, E>`)",
            ));
        }

        // The Pascal declaration
        let mut scope = Scope::default();
        let mut texts = Vec::new();
        for p in &params {
            if let Some(n) = &p.pascal {
                param_name(n, span, &mut scope)?;
                texts.push(param_text(n, &p.ty));
            }
        }
        let plist = if texts.is_empty() {
            String::new()
        } else {
            format!("({})", texts.join("; "))
        };
        let instance = this != This::None;
        let decl = if is_ctor {
            format!("constructor {name}{plist}; native;")
        } else {
            let prefix = if class.is_some() && !instance {
                "class "
            } else {
                ""
            };
            match ret.pascal() {
                Some(r) => format!("{prefix}function {name}{plist}: {r}; native;"),
                None => format!("{prefix}procedure {name}{plist}; native;"),
            }
        };
        if let Some((_, cname)) = class {
            let ci = self
                .classes
                .iter()
                .position(|c| &c.name == cname)
                .expect("the class was read");
            self.classes[ci]
                .scope
                .declare(&name, "the member", sig.ident.span())?;
            for (key, g) in [(&args.getter, true), (&args.setter, false)] {
                if let Some(p) = key {
                    self.property(ci, p, g, &name, instance, &params, &ret)?;
                }
            }
        } else {
            self.scope.declare(&name, "the name", sig.ident.span())?;
        }

        // The call
        let reg_name = match class {
            Some((_, c)) => format!("{c}.{name}"),
            None => name.clone(),
        };
        let wrapper = match class {
            Some((r, _)) => format_ident!("__puy_{}_{}", r, strip_raw(&rust_name)),
            None => format_ident!("__puy_fn_{}", strip_raw(&rust_name)),
        };
        let fn_ident = &sig.ident;
        let path = match class {
            Some((r, _)) => quote!(#r::#fn_ident),
            None => quote!(#fn_ident),
        };
        let copy = is_async || takes_ctx;
        let mut owned = Vec::new();
        let mut borrowed = Vec::new();
        let mut passed = Vec::new();
        if matches!(this, This::Ref | This::RefMut) {
            let r = &class.expect("a method").0;
            owned.push(quote!(let __this = __ctx.this::<#r>()?;));
        }
        let mut index = usize::from(matches!(this, This::Ref | This::RefMut));
        for (k, p) in params.iter().enumerate() {
            let a = format_ident!("__a{}", k);
            if p.ty == Ty::Ctx {
                passed.push(quote!(&mut *__ctx));
                continue;
            }
            let i = index;
            index += 1;
            match (&p.ty, copy) {
                (Ty::StrRef, false) => {
                    borrowed.push(quote!(let #a = __ctx.utf8(#i)?;));
                    passed.push(quote!(&*#a));
                }
                (Ty::StrRef, true) => {
                    owned.push(quote!(let #a = __ctx.string(#i)?;));
                    passed.push(quote!(&*#a));
                }
                (Ty::PasStr, false) => {
                    borrowed.push(quote!(let #a = __ctx.str(#i)?;));
                    passed.push(quote!(#a));
                }
                (Ty::PasStr, true) => {
                    owned.push(quote!(let #a = __ctx.str(#i)?.to_pas_string();));
                    passed.push(quote!(#a.as_pas_str()));
                }
                (Ty::BytesRef, false) => {
                    borrowed.push(quote!(let #a = __ctx.bytes(#i)?;));
                    passed.push(quote!(&*#a));
                }
                (Ty::BytesRef, true) => {
                    owned.push(quote!(let #a = __ctx.bytes(#i)?.into_owned();));
                    passed.push(quote!(&*#a));
                }
                // Copied element by element, whether the routine calls back or not
                (Ty::SliceRef(_), _) => {
                    owned.push(quote!(let #a = ::puy_ext::__private::slice_arg(__ctx, #i)?;));
                    passed.push(quote!(#a.as_slice()));
                }
                _ => {
                    owned.push(
                        quote!(let #a = ::puy_ext::__private::FromArg::from_arg(__ctx, #i)?;),
                    );
                    passed.push(quote!(#a));
                }
            }
        }
        let guard = match this {
            This::Ref => {
                passed.insert(0, quote!(&*__guard));
                quote!(let __guard = __this.borrow()?;)
            }
            This::RefMut => {
                passed.insert(0, quote!(&mut *__guard));
                quote!(let mut __guard = __this.borrow_mut()?;)
            }
            _ => quote!(),
        };
        // A `Vec` other than `Vec<u8>` is made a value element by element
        let convert = match (is_result, matches!(ret, Ty::Array(_))) {
            (true, false) => quote!(::puy_ext::__private::result(__r)),
            (false, false) => quote!(::puy_ext::__private::ok(__r)),
            (true, true) => quote!(::puy_ext::__private::result_value(__r)),
            (false, true) => quote!(::puy_ext::__private::ok_value(__r)),
        };
        let body = if is_async {
            quote! {
                #(#owned)*
                ::puy_ext::__private::future(async move {
                    let __r = #path(#(#passed),*).await;
                    #convert
                })
            }
        } else if started.is_some() {
            // Borrowed arguments are read in the call, before the work is given back
            quote! {
                #(#owned)*
                #(#borrowed)*
                #guard
                let __f = #path(#(#passed),*);
                ::puy_ext::__private::future(async move {
                    let __r = __f.await;
                    #convert
                })
            }
        } else {
            quote! {
                #(#owned)*
                #(#borrowed)*
                #guard
                let __r = #path(#(#passed),*);
                #convert
            }
        };
        let out = if suspends {
            quote!(::puy_ext::ExtFuture)
        } else {
            quote!(::puy_ext::Value)
        };
        self.wrappers.push(quote! {
            #[doc(hidden)]
            #[allow(clippy::all, non_snake_case)]
            fn #wrapper(
                __ctx: &mut ::puy_ext::CallCtx<'_>,
            ) -> ::core::result::Result<#out, ::puy_ext::PasError> {
                #body
            }
        });
        self.registered.push(Routine {
            name: reg_name,
            wrapper,
            is_async: suspends,
            cancel,
        });
        Ok(decl)
    }

    /// Adds the method `method` as the getter (`getter`) or setter of the property `prop`.
    #[allow(clippy::too_many_arguments)]
    fn property(
        &mut self,
        ci: usize,
        prop: &LitStr,
        getter: bool,
        method: &str,
        instance: bool,
        params: &[Param],
        ret: &Ty,
    ) -> syn::Result<()> {
        let span = prop.span();
        let name = prop.value();
        check_ident(&name, span)?;
        let pascal: Vec<&Param> = params.iter().filter(|p| p.pascal.is_some()).collect();
        let ty = if getter {
            match (instance, pascal.len(), ret.pascal()) {
                (true, 0, Some(t)) => t,
                _ => {
                    return Err(syn::Error::new(
                        span,
                        "a getter is a method of the instance without parameters that returns \
                         the property's value",
                    ));
                }
            }
        } else {
            match (instance, pascal.as_slice(), ret) {
                (true, [p], Ty::Unit) => p.ty.pascal().expect("a Pascal parameter"),
                _ => {
                    return Err(syn::Error::new(
                        span,
                        "a setter is a method of the instance that takes the value and returns \
                         nothing",
                    ));
                }
            }
        };
        let class = &mut self.classes[ci];
        let p = match class
            .properties
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(&name))
        {
            Some(p) => p,
            None => {
                class.scope.declare(&name, "the member", span)?;
                class.properties.push(Property {
                    name: name.clone(),
                    ty: ty.clone(),
                    getter: None,
                    setter: None,
                });
                class.properties.last_mut().expect("just pushed")
            }
        };
        if p.ty != ty {
            return Err(syn::Error::new(
                span,
                format!(
                    "the getter and the setter of `{name}` disagree on its type (`{}` and `{ty}`)",
                    p.ty
                ),
            ));
        }
        let slot = if getter { &mut p.getter } else { &mut p.setter };
        if slot.is_some() {
            return Err(syn::Error::new(
                span,
                format!(
                    "`{name}` has two {}s",
                    if getter { "getter" } else { "setter" }
                ),
            ));
        }
        *slot = Some(method.to_string());
        Ok(())
    }

    /// The unit's source: its `interface` part and an empty `implementation` part.
    fn source(&self, unit: &str) -> String {
        let mut s = format!("unit {unit};\n\ninterface\n");
        if !self.consts.is_empty() {
            s.push_str("\nconst\n");
            for c in &self.consts {
                write_doc(&mut s, &c.doc, "  ");
                s.push_str(&format!("  {} = {};\n", c.name, c.value));
            }
        }
        let has_types = !(self.exceptions.is_empty()
            && self.enums.is_empty()
            && self.records.is_empty()
            && self.callbacks.is_empty()
            && self.classes.is_empty());
        if has_types {
            s.push_str("\ntype\n");
            for e in &self.exceptions {
                write_doc(&mut s, &e.doc, "  ");
                s.push_str(&format!("  {} = class({});\n", e.name, e.parent));
            }
            for e in &self.enums {
                write_doc(&mut s, &e.doc, "  ");
                let values: Vec<&str> = e.values.iter().map(|(_, n)| n.as_str()).collect();
                s.push_str(&format!("  {} = ({});\n", e.name, values.join(", ")));
            }
            for r in &self.records {
                write_doc(&mut s, &r.doc, "  ");
                s.push_str(&format!("  {} = record\n", r.name));
                for (doc, _, n, t) in &r.fields {
                    write_doc(&mut s, doc, "    ");
                    s.push_str(&format!("    {n}: {t};\n"));
                }
                s.push_str("  end;\n");
            }
            // Declared ahead, so that callbacks and classes can name any class
            for c in &self.classes {
                s.push_str(&format!("  {} = class;\n", c.name));
            }
            for c in &self.callbacks {
                write_doc(&mut s, &c.doc, "  ");
                let params = if c.params.is_empty() {
                    String::new()
                } else {
                    format!("({})", c.params)
                };
                match &c.ret {
                    Some(r) => s.push_str(&format!(
                        "  {} = reference to function{params}: {r};\n",
                        c.name
                    )),
                    None => {
                        s.push_str(&format!("  {} = reference to procedure{params};\n", c.name))
                    }
                }
            }
            for c in &self.classes {
                s.push('\n');
                write_doc(&mut s, &c.doc, "  ");
                s.push_str(&format!(
                    "  {} = class sealed(TRefObject)\n  public\n",
                    c.name
                ));
                for (doc, decl) in &c.members {
                    write_doc(&mut s, doc, "    ");
                    s.push_str(&format!("    {decl}\n"));
                }
                for p in &c.properties {
                    let mut line = format!("    property {}: {}", p.name, p.ty);
                    if let Some(g) = &p.getter {
                        line.push_str(&format!(" read {g}"));
                    }
                    if let Some(w) = &p.setter {
                        line.push_str(&format!(" write {w}"));
                    }
                    s.push_str(&line);
                    s.push_str(";\n");
                }
                s.push_str("  end;\n");
            }
        }
        if !self.routines.is_empty() {
            s.push('\n');
            for (doc, decl) in &self.routines {
                write_doc(&mut s, doc, "");
                s.push_str(decl);
                s.push('\n');
            }
        }
        s.push_str("\nimplementation\n\nend.\n");
        s
    }

    /// The items added to the module: the registration table, the calls, the conversions.
    fn generate(&self, args: &ModuleArgs, source: &str) -> Vec<Item> {
        let unit = &args.unit;
        let functions = self.registered.iter().map(|r| {
            let (name, w, cancel) = (&r.name, &r.wrapper, &r.cancel);
            if r.is_async {
                quote!(::puy_ext::Function::suspendable(#name, #w, #cancel))
            } else {
                quote!(::puy_ext::Function::sync(#name, #w))
            }
        });
        let classes = self.classes.iter().map(|c| {
            let (r, n) = (&c.rust, &c.name);
            quote!(::puy_ext::Class::of::<#r>(#n))
        });
        let caps = &args.capabilities;
        let wrappers = &self.wrappers;
        let class_values = self.classes.iter().map(|c| {
            let r = &c.rust;
            quote! {
                impl ::core::convert::From<#r> for ::puy_ext::Value {
                    fn from(v: #r) -> ::puy_ext::Value {
                        ::puy_ext::Value::object(v)
                    }
                }
            }
        });
        let exceptions = self.exceptions.iter().map(|e| {
            let (r, n) = (&e.rust, &e.name);
            quote! {
                impl ::puy_ext::PascalException for #r {
                    const NAME: &'static str = #n;
                }
            }
        });
        let enums = self.enums.iter().map(|e| {
            let (r, n) = (&e.rust, &e.name);
            let idents: Vec<&syn::Ident> = e.values.iter().map(|(i, _)| i).collect();
            let ords: Vec<i64> = (0..idents.len() as i64).collect();
            quote! {
                impl ::puy_ext::__private::FromArg for #r {
                    fn from_arg(
                        ctx: &mut ::puy_ext::CallCtx<'_>,
                        i: usize,
                    ) -> ::core::result::Result<#r, ::puy_ext::PasError> {
                        match ::puy_ext::__private::enum_ordinal(ctx, i)? {
                            #(#ords => Ok(#r::#idents),)*
                            v => Err(::puy_ext::__private::enum_out_of_range(#n, v)),
                        }
                    }
                }
                impl ::core::convert::From<#r> for ::puy_ext::Value {
                    fn from(v: #r) -> ::puy_ext::Value {
                        ::puy_ext::Value::Int(match v {
                            #(#r::#idents => #ords,)*
                        })
                    }
                }
                impl ::puy_ext::__private::FromValue for #r {
                    fn from_value(
                        v: ::puy_ext::Value,
                    ) -> ::core::result::Result<#r, ::puy_ext::PasError> {
                        match ::puy_ext::__private::int_value(v)? {
                            #(#ords => Ok(#r::#idents),)*
                            v => Err(::puy_ext::__private::enum_out_of_range(#n, v)),
                        }
                    }
                }
                impl ::puy_ext::__private::IntoValue for #r {
                    fn into_value(self) -> ::puy_ext::Value {
                        self.into()
                    }
                }
            }
        });
        let records = self.records.iter().map(|r| {
            let rust = &r.rust;
            let idents: Vec<&syn::Ident> = r.fields.iter().map(|(_, i, _, _)| i).collect();
            let n = idents.len();
            quote! {
                impl ::puy_ext::__private::FromValue for #rust {
                    fn from_value(
                        v: ::puy_ext::Value,
                    ) -> ::core::result::Result<#rust, ::puy_ext::PasError> {
                        #[allow(unused_mut, unused_variables)]
                        let mut __f = ::puy_ext::__private::record_fields(v, #n)?;
                        Ok(#rust {
                            #(#idents: ::puy_ext::__private::field(&mut __f)?,)*
                        })
                    }
                }
                impl ::puy_ext::__private::IntoValue for #rust {
                    fn into_value(self) -> ::puy_ext::Value {
                        ::puy_ext::Value::Record(::std::vec![
                            #(::puy_ext::__private::IntoValue::into_value(self.#idents)),*
                        ])
                    }
                }
                impl ::core::convert::From<#rust> for ::puy_ext::Value {
                    fn from(v: #rust) -> ::puy_ext::Value {
                        ::puy_ext::__private::IntoValue::into_value(v)
                    }
                }
                impl ::puy_ext::__private::FromArg for #rust {
                    fn from_arg(
                        ctx: &mut ::puy_ext::CallCtx<'_>,
                        i: usize,
                    ) -> ::core::result::Result<#rust, ::puy_ext::PasError> {
                        ::puy_ext::__private::value_arg(ctx, i)
                    }
                }
            }
        });
        let tokens = quote! {
            /// The source of the unit the attributes generated: its `interface` part.
            pub const SOURCE: &str = #source;

            /// The registration table of the unit.
            pub static EXTENSION: ::puy_ext::Extension = ::puy_ext::Extension {
                unit: #unit,
                source: SOURCE,
                fingerprint: ::puy_ext::fingerprint(SOURCE),
                functions: &[#(#functions),*],
                classes: &[#(#classes),*],
                capabilities: &[#(#caps),*],
            };

            /// The registration table of the unit, to add to an `ExtensionSet`.
            pub fn extension() -> &'static ::puy_ext::Extension {
                &EXTENSION
            }

            #(#wrappers)*
            #(#class_values)*
            #(#exceptions)*
            #(#enums)*
            #(#records)*
        };
        let file: syn::File = syn::parse2(tokens).expect("the generated items parse");
        file.items
    }
}

/// Whether a callable value's signature can have a parameter or result of this type
/// (`docs/spec/extensions.md` 3: neither `TBytes` nor a callable value).
fn callback_type(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Scalar(_) | Ty::Text(_) | Ty::StrRef | Ty::Obj { .. } | Ty::Enum(_) | Ty::Unit
    )
}

/// Checks a parameter's Pascal name.
fn param_name(name: &str, span: Span, scope: &mut Scope) -> syn::Result<()> {
    check_ident(name, span)?;
    if name.eq_ignore_ascii_case("Result") || name.eq_ignore_ascii_case("Self") {
        return Err(syn::Error::new(
            span,
            format!("a parameter named `{name}` hides the routine's own `{name}`"),
        ));
    }
    scope.declare(name, "the parameter", span)
}

/// A parameter as Pascal writes it: `const S: string`, `X: Integer`.
fn param_text(name: &str, ty: &Ty) -> String {
    let t = ty.pascal().expect("a Pascal parameter");
    if ty.is_const_param() {
        format!("const {name}: {t}")
    } else {
        format!("{name}: {t}")
    }
}

fn no_generics(g: &syn::Generics, what: &str) -> syn::Result<()> {
    if g.params.is_empty() && g.where_clause.is_none() {
        return Ok(());
    }
    Err(syn::Error::new(
        g.span(),
        format!("{what} is not generic: Pascal sees only concrete types (requirements §6)"),
    ))
}

fn strip_raw(s: &str) -> &str {
    s.strip_prefix("r#").unwrap_or(s)
}

/// A constant's value as a Pascal literal: an integer, a real, a string or a `bool`.
fn pascal_literal(e: &Expr) -> syn::Result<String> {
    let err = || {
        Err(syn::Error::new(
            e.span(),
            "a `#[pascal_const]` is a literal: an integer, a real, a string or `true` / `false`",
        ))
    };
    match e {
        Expr::Lit(l) => match &l.lit {
            Lit::Int(i) => Ok(i.base10_digits().to_string()),
            Lit::Float(f) => {
                let mut d = f.base10_digits().to_string();
                if d.ends_with('.') {
                    d.push('0');
                }
                if !d.contains(['.', 'e', 'E']) {
                    d.push_str(".0");
                }
                Ok(d)
            }
            Lit::Str(s) => Ok(pascal_string(&s.value())),
            Lit::Bool(b) => Ok(if b.value { "True" } else { "False" }.to_string()),
            _ => err(),
        },
        Expr::Unary(u) if matches!(u.op, UnOp::Neg(_)) => match &*u.expr {
            Expr::Lit(l) if matches!(l.lit, Lit::Int(_) | Lit::Float(_)) => {
                Ok(format!("-{}", pascal_literal(&u.expr)?))
            }
            _ => err(),
        },
        Expr::Group(g) => pascal_literal(&g.expr),
        Expr::Paren(p) => pascal_literal(&p.expr),
        _ => err(),
    }
}

/// `s` as a Pascal string literal: quoted, with `'` doubled and control characters as `#N`.
fn pascal_string(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    let mut out = String::new();
    let mut quoted = false;
    for c in s.chars() {
        if c.is_control() {
            if quoted {
                out.push('\'');
                quoted = false;
            }
            out.push_str(&format!("#{}", c as u32));
        } else {
            if !quoted {
                out.push('\'');
                quoted = true;
            }
            if c == '\'' {
                out.push('\'');
            }
            out.push(c);
        }
    }
    if quoted {
        out.push('\'');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The unit source `#[pascal_module(attr)]` generates for the module `item`, or the error.
    fn source(attr: TokenStream, item: TokenStream) -> String {
        let mut m: ItemMod = syn::parse2(item).expect("a module");
        match expand(attr, &mut m) {
            Ok((_, s)) => s,
            Err(e) => format!("error: {e}"),
        }
    }

    // A plain `fn` returning `impl Future` takes the `CallCtx` (to start host calls) and is
    // registered as suspendable, with its `cancel`; its declaration is that of the `Output`.
    #[test]
    fn started_routine_is_suspendable() {
        let mut m: ItemMod = syn::parse2(quote! {
            mod m {
                #[pascal_function(cancel = "complete")]
                pub fn exists(
                    ctx: &mut CallCtx<'_>,
                    path: &str,
                ) -> impl Future<Output = Result<bool, PasError>> + 'static {
                    async { Ok(true) }
                }
            }
        })
        .expect("a module");
        let (tokens, source) = expand(quote!(unit = "Demo.Fs"), &mut m).expect("expands");
        assert!(
            source.contains("function Exists(const Path: string): Boolean; native;"),
            "{source}"
        );
        let tokens = tokens.to_string().replace(' ', "");
        assert!(
            tokens.contains(
                "Function::suspendable(\"Exists\",__puy_fn_exists,::puy_ext::CancelMode::Complete)"
            ),
            "{tokens}"
        );
    }

    // A plain `fn` returning `impl Future` without the `CallCtx` borrows its arguments in the
    // call: the borrowed bytes are read before the work is started (they were not bound).
    #[test]
    fn started_routine_binds_borrowed_arguments() {
        let mut m: ItemMod = syn::parse2(quote! {
            mod m {
                #[pascal_function]
                pub fn put(bytes: &[u8]) -> impl Future<Output = Result<i32, PasError>> + 'static {
                    let n = bytes.len() as i32;
                    async move { Ok(n) }
                }
            }
        })
        .expect("a module");
        let (tokens, _) = expand(quote!(unit = "Demo.Put"), &mut m).expect("expands");
        let tokens = tokens.to_string().replace(' ', "");
        assert!(
            tokens.contains("let__a0=__ctx.bytes(0usize)?;let__f=put(&*__a0);"),
            "{tokens}"
        );
    }

    // The declarations of each kind of item, in the order of the unit: constants, exceptions,
    // enumerations, classes declared ahead, callback types, classes, routines; `///` comments
    // carried over.
    #[test]
    fn generates_the_unit() {
        let s = source(
            quote!(unit = "Demo.Zip"),
            quote! {
                mod zip {
                    /// The largest entry.
                    #[pascal_const]
                    pub const MAX_ENTRY: i64 = -5;
                    #[pascal_const(name = "Greeting")]
                    pub const HELLO: &str = "it's\n";
                    #[pascal_exception(name = "EZipError", parent = EArgumentException)]
                    pub struct ZipError;
                    #[pascal_enum(name = "TMethod")]
                    pub enum Method { Stored, #[pascal(name = "Deflated")] Deflate }
                    /// Called for each entry.
                    #[pascal_callback(name = "TEntryFunc")]
                    pub type EntryFunc = fn(name: &str, size: i64) -> bool;
                    /// An open archive.
                    #[pascal_class(name = "TZipArchive")]
                    pub struct Archive { n: i32 }
                    #[pascal_methods]
                    impl Archive {
                        #[pascal_constructor]
                        pub fn new(path: &str) -> Result<Self, PasError> { todo!() }
                        /// The number of entries.
                        #[pascal_function(getter = "Count")]
                        pub fn entry_count(&self) -> i32 { self.n }
                        pub fn each(&mut self, ctx: &mut CallCtx<'_>, f: PasFn<EntryFunc>) {}
                        pub fn same(this: PasObj<Self>) -> PasObj<Self> { this }
                        pub fn version() -> String { String::new() }
                        #[pascal_function(cancel = "complete")]
                        pub async fn extract(this: PasObj<Self>, dest: String) {}
                        fn helper(&self) {}
                    }
                    #[pascal_function]
                    pub fn crc32(data: &[u8], method: Method) -> u32 { 0 }
                    #[pascal_function(name = "FailIO")]
                    pub fn fail_io(code: i32) -> Result<(), PasError> { Ok(()) }
                    pub fn not_exported() {}
                }
            },
        );
        assert_eq!(
            s,
            "unit Demo.Zip;

interface

const
  /// The largest entry.
  MaxEntry = -5;
  Greeting = 'it''s'#10;

type
  EZipError = class(EArgumentException);
  TMethod = (Stored, Deflated);
  TZipArchive = class;
  /// Called for each entry.
  TEntryFunc = reference to function(const Name: string; Size: Int64): Boolean;

  /// An open archive.
  TZipArchive = class sealed(TRefObject)
  public
    constructor Create(const Path: string); native;
    /// The number of entries.
    function EntryCount: Integer; native;
    procedure Each(const F: TEntryFunc); native;
    function Same: TZipArchive; native;
    class function Version: string; native;
    procedure Extract(const Dest: string); native;
    property Count: Integer read EntryCount;
  end;

function Crc32(const Data: TArray<Byte>; Method: TMethod): Cardinal; native;
procedure FailIO(Code: Integer); native;

implementation

end.
"
        );
    }

    // Records come after the enumerations and before the classes, in the order they are written,
    // a field naming an earlier record; `Vec` and `&[T]` are `TArray<T>`.
    #[test]
    fn generates_records_and_arrays() {
        let s = source(
            quote!(unit = "Demo.Find"),
            quote! {
                mod find {
                    #[pascal_enum(name = "TKind")]
                    pub enum Kind { Plain, Dir }
                    /// A match.
                    #[pascal_record(name = "TSpan")]
                    pub struct Span {
                        /// Where it starts.
                        pub start: i64,
                        #[pascal(name = "Len")]
                        pub length: i32,
                    }
                    #[pascal_record(name = "TEntry")]
                    pub struct Entry {
                        name: String,
                        kind: Kind,
                        spans: Vec<Span>,
                        data: Vec<u8>,
                    }
                    #[pascal_class(name = "TFinder")]
                    pub struct Finder;
                    #[pascal_methods]
                    impl Finder {
                        pub fn last(&self) -> Span { todo!() }
                    }
                    #[pascal_function]
                    pub fn find(names: &[String], at: Span) -> Vec<Entry> { Vec::new() }
                    #[pascal_function]
                    pub fn sizes(x: Vec<Vec<f64>>) -> Result<Vec<i64>, PasError> { Ok(Vec::new()) }
                }
            },
        );
        assert_eq!(
            s,
            "unit Demo.Find;

interface

type
  TKind = (Plain, Dir);
  /// A match.
  TSpan = record
    /// Where it starts.
    Start: Int64;
    Len: Integer;
  end;
  TEntry = record
    Name: string;
    Kind: TKind;
    Spans: TArray<TSpan>;
    Data: TArray<Byte>;
  end;
  TFinder = class;

  TFinder = class sealed(TRefObject)
  public
    function Last: TSpan; native;
  end;

function Find(const Names: TArray<string>; const At: TSpan): TArray<TEntry>; native;
function Sizes(const X: TArray<TArray<Double>>): TArray<Int64>; native;

implementation

end.
"
        );
    }

    // A getter and a setter make one property.
    #[test]
    fn getter_and_setter() {
        let s = source(
            quote!(unit = "P"),
            quote! {
                mod p {
                    #[pascal_class(name = "TBox")]
                    pub struct Box1;
                    #[pascal_methods]
                    impl Box1 {
                        #[pascal_function(getter = "Value")]
                        pub fn get_value(&self) -> f64 { 0.0 }
                        #[pascal_function(setter = "Value")]
                        pub fn set_value(&mut self, v: f64) {}
                    }
                }
            },
        );
        assert!(
            s.contains("    property Value: Double read GetValue write SetValue;\n"),
            "{s}"
        );
    }

    // The rules the attributes check, each with its message.
    #[test]
    fn refuses_what_pascal_cannot_see() {
        for (item, msg) in [
            (
                quote!(
                    mod m {
                        #[pascal_function]
                        pub fn r#begin() {}
                    }
                ),
                "`Begin` is a reserved word",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_function]
                        pub fn f(x: usize) {}
                    }
                ),
                "`usize` has no Pascal counterpart",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_function]
                        pub fn f<T>(x: T) {}
                    }
                ),
                "is not generic",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_function]
                        pub fn f(result: i32) {}
                    }
                ),
                "hides the routine's own `Result`",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_function]
                        pub fn add() {}
                        #[pascal_function(name = "ADD")]
                        pub fn add2() {}
                    }
                ),
                "`ADD` is declared twice",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_class]
                        pub struct C;
                    }
                ),
                "a class is named in Pascal",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_record]
                        pub struct R {
                            x: i32,
                        }
                    }
                ),
                "a record is named in Pascal",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_record(name = "TR")]
                        pub struct R(i32);
                    }
                ),
                "a record's fields are named",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_record(name = "TR")]
                        pub struct R {
                            s: &'static str,
                        }
                    }
                ),
                "a field of a record or an element of a dynamic array is",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_record(name = "TR")]
                        pub struct R {
                            next: Vec<S>,
                        }
                        #[pascal_record(name = "TS")]
                        pub struct S {
                            x: i32,
                        }
                    }
                ),
                "type `S` has no Pascal counterpart",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_record(name = "TR")]
                        pub struct R {
                            x: i32,
                            #[pascal(name = "X")]
                            y: i32,
                        }
                    }
                ),
                "`X`",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_record(name = "TR")]
                        pub struct R {
                            x: i32,
                        }
                        #[pascal_callback(name = "TF")]
                        pub type F = fn(r: R);
                    }
                ),
                "a callable value's parameters are",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_function(cancel = "drop")]
                        pub fn f() {}
                    }
                ),
                "for an `async` routine only",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_class(name = "TC")]
                        pub struct C;
                        #[pascal_methods]
                        impl C {
                            pub async fn f(&self) {}
                        }
                    }
                ),
                "cannot borrow `self` across `await`",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_function]
                        pub async fn f(ctx: &mut CallCtx<'_>) {}
                    }
                ),
                "cannot take the `CallCtx`",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_function]
                        pub fn f() -> &'static str {
                            ""
                        }
                    }
                ),
                "returns owned values",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_class(name = "TC")]
                        pub struct C;
                        #[pascal_methods]
                        impl C {
                            #[pascal_constructor]
                            pub fn new() -> i32 {
                                0
                            }
                        }
                    }
                ),
                "a constructor returns `Self`",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_const]
                        pub const X: [i32; 1] = [1];
                    }
                ),
                "a `#[pascal_const]` is a literal",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_callback(name = "TF")]
                        pub type F = fn(b: Vec<u8>);
                    }
                ),
                "a callable value's parameters",
            ),
            (
                quote!(
                    mod m;
                ),
                "needs the module's items inline",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_exception(name = "EZipCrc", parent = EZipError)]
                        pub struct Crc;
                        #[pascal_exception(name = "EZipError")]
                        pub struct Zip;
                    }
                ),
                "the parent `EZipError` of `EZipCrc` is declared after it",
            ),
            (
                quote!(
                    mod m {
                        #[pascal_exception(name = "EFoo", parent = efoo)]
                        pub struct Foo;
                    }
                ),
                "the exception `EFoo` is its own parent",
            ),
        ] {
            let s = source(quote!(unit = "M"), item.clone());
            assert!(s.contains(msg), "{item}: {s}");
        }
        assert!(
            source(
                quote!(),
                quote!(
                    mod m {}
                )
            )
            .contains("needs the unit's name")
        );
        assert!(
            source(
                quote!(unit = "A.Type"),
                quote!(
                    mod m {}
                )
            )
            .contains("reserved word")
        );
    }

    // Quotes are doubled and control characters written as `#N`.
    #[test]
    fn pascal_string_literals() {
        assert_eq!(pascal_string(""), "''");
        assert_eq!(pascal_string("a'b"), "'a''b'");
        assert_eq!(pascal_string("\tx\n"), "#9'x'#10");
        assert_eq!(pascal_string("日本"), "'日本'");
    }
}
