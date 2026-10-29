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
    // With a direct `uniffi` dependency the alias would make `uniffi` ambiguous (E0659).
    let alias =
        (!has_direct_uniffi_dependency()).then(|| quote! { extern crate eqswift as uniffi; });
    quote! {
        #alias
        ::eqswift::setup_scaffolding!();
    }
    .into()
}

/// Whether the crate being compiled lists `uniffi` under `[dependencies]` or
/// `[target.*.dependencies]` (the deps a library build gets).
fn has_direct_uniffi_dependency() -> bool {
    let Some(manifest) = std::env::var_os("CARGO_MANIFEST_DIR")
        .and_then(|dir| std::fs::read_to_string(std::path::Path::new(&dir).join("Cargo.toml")).ok())
        .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
    else {
        return false;
    };
    let lists_uniffi = |table: &toml::Table| {
        table
            .get("dependencies")
            .and_then(|deps| deps.get("uniffi"))
            .is_some()
    };
    lists_uniffi(&manifest)
        || manifest
            .get("target")
            .and_then(|targets| targets.as_table())
            .is_some_and(|targets| {
                targets
                    .values()
                    .any(|t| t.as_table().is_some_and(lists_uniffi))
            })
}
