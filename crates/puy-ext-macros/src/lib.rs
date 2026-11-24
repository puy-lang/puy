//! The attributes that write a Puy extension: `#[pascal_module]` on an inline module reads the
//! items inside it marked with the other attributes, and generates the unit's Pascal
//! declarations (its `interface` part), the calls of the Rust functions and the registration
//! table (requirements §6; `docs/spec/extensions.md` chapter 7). Use them through `puy-ext`,
//! which re-exports them.
//!
//! The attributes other than `#[pascal_module]` are read by it, and are errors anywhere else.

use proc_macro::TokenStream;

mod expand;
mod names;
mod reserved;
mod types;

/// A unit written in Rust: `#[pascal_module(unit = "Puy.Zip")] mod zip { … }`, with
/// `capabilities(fs, net)` for an extension that uses the OS outside the host interface.
///
/// Adds to the module `SOURCE` (the unit's source), `EXTENSION` (the registration table) and
/// `extension()`.
#[proc_macro_attribute]
pub fn pascal_module(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::module(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// The attribute `name` outside a `#[pascal_module]`.
fn outside(name: &str, item: TokenStream) -> TokenStream {
    let item = proc_macro2::TokenStream::from(item);
    let msg = format!("`#[{name}]` is read by `#[pascal_module]`: write it inside such a module");
    let err = syn::Error::new(proc_macro2::Span::call_site(), msg).into_compile_error();
    quote::quote!(#err #item).into()
}

macro_rules! inner_attributes {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[proc_macro_attribute]
        pub fn $name(_attr: TokenStream, item: TokenStream) -> TokenStream {
            outside(stringify!($name), item)
        }
    )*};
}

inner_attributes! {
    /// A routine: `#[pascal_function]`, with `name = "…"` (the Pascal name; `snake_case`
    /// becomes `PascalCase` by default) and, for an `async fn`, `cancel = "drop" | "detach" |
    /// "complete"`. On a method, also `getter = "Prop"` / `setter = "Prop"`.
    pascal_function,
    /// A struct whose values are the instances of a reference-counted class:
    /// `#[pascal_class(name = "TZipArchive")]`.
    pascal_class,
    /// The methods of a `#[pascal_class]` struct: its public methods are exported.
    pascal_methods,
    /// A constructor among the methods (`name` defaults to `Create` for `new`).
    pascal_constructor,
    /// An exception class: `#[pascal_exception(name = "EZipError", parent = EArgumentException)]`
    /// on a unit struct.
    pascal_exception,
    /// An enumeration: `#[pascal_enum(name = "TMethod")]` on an enum without fields.
    pascal_enum,
    /// A constant: `#[pascal_const]` on a `const` whose value is a literal.
    pascal_const,
    /// The type of a callable value: `#[pascal_callback(name = "TIntFunc")]` on
    /// `type IntFunc = fn(x: i32) -> i32;`, passed as `PasFn<IntFunc>`.
    pascal_callback,
    /// A record (not passed to `native` routines yet).
    pascal_record,
}
