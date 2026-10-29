use proc_macro::TokenStream;
use quote::quote;

/// Export a function, method, impl block, or trait to foreign languages.
///
/// Thin wrapper around `#[uniffi::export]`:
///
/// ```ignore
/// #[eqswift::export]
/// fn hello(name: String) -> String {
///     format!("Hello, {name}!")
/// }
/// ```
///
/// For constructors inside exported impl blocks, use `#[uniffi::constructor]`:
///
/// ```ignore
/// #[derive(eqswift::Object)]
/// pub struct Greeter;
///
/// #[eqswift::export]
/// impl Greeter {
///     #[uniffi::constructor]
///     pub fn new() -> Self { Self }
///
///     pub fn greet(&self, name: String) -> String {
///         format!("Hello, {name}!")
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn export(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = proc_macro2::TokenStream::from(args);
    let input = proc_macro2::TokenStream::from(input);

    quote! {
        #[::eqswift::__uniffi_export(#args)]
        #input
    }
    .into()
}

/// One-time setup macro.
///
/// Expands to `uniffi::setup_scaffolding!()`. Call **once** at the top of your
/// crate root (`lib.rs`) before any exported items.
///
/// Unless the crate's `Cargo.toml` already depends on `uniffi`, it also adds
/// `extern crate eqswift as uniffi;` to the crate root. Root `extern crate` items join the
/// extern prelude, so the `::uniffi::...` paths UniFFI's macros generate (and
/// `#[uniffi::constructor]`) resolve through `eqswift`, and no direct `uniffi` dependency is needed.
///
/// ```ignore
/// eqswift::setup!();
///
/// #[eqswift::export]
/// pub fn add(a: u32, b: u32) -> u32 { a + b }
/// ```
#[proc_macro]
pub fn setup(_input: TokenStream) -> TokenStream {
    let alias = uniffi_alias();
    quote! {
        #alias
        ::eqswift::setup_scaffolding!();
    }
    .into()
}

/// `extern crate eqswift as uniffi;`, gated off wherever the crate gets a real `uniffi`, since
/// both at once make `uniffi` ambiguous (E0659):
/// - `[dependencies] uniffi`: no alias at all;
/// - `[target.'cfg(p)'.dependencies] uniffi`: `#[cfg(not(p))]`, so other targets keep the alias;
/// - `[dev-dependencies] uniffi`: `#[cfg(not(test))]`, since dev-deps reach the lib only in its
///   own test build.
///
/// A target table keyed by a triple (`[target.x86_64-apple-darwin.dependencies]`) can't be turned
/// into a `cfg` here, so it is treated as always active, as before.
fn uniffi_alias() -> Option<proc_macro2::TokenStream> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .and_then(|dir| std::fs::read_to_string(std::path::Path::new(&dir).join("Cargo.toml")).ok())
        .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
        .unwrap_or_default();
    let lists = |table: &toml::Table, section: &str| {
        table
            .get(section)
            .and_then(|deps| deps.get("uniffi"))
            .is_some()
    };
    if lists(&manifest, "dependencies") {
        return None;
    }
    let mut gates = Vec::new();
    for (key, table) in manifest
        .get("target")
        .and_then(|targets| targets.as_table())
        .into_iter()
        .flatten()
    {
        let Some(table) = table.as_table() else {
            continue;
        };
        if !lists(table, "dependencies") {
            continue;
        }
        let predicate = key
            .strip_prefix("cfg(")
            .and_then(|rest| rest.strip_suffix(')'))
            .and_then(|inner| inner.parse::<proc_macro2::TokenStream>().ok());
        // A triple key has no `cfg` form: treat it as active, so no alias.
        gates.push(predicate?);
    }
    if lists(&manifest, "dev-dependencies") {
        gates.push(quote! { test });
    }
    let alias = quote! { extern crate eqswift as uniffi; };
    Some(if gates.is_empty() {
        alias
    } else {
        quote! { #[cfg(not(any(#(#gates),*)))] #alias }
    })
}
