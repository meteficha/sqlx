#![cfg_attr(
    not(any(feature = "postgres", feature = "mysql", feature = "offline")),
    allow(dead_code, unused_macros, unused_imports)
)]
extern crate proc_macro;

use proc_macro::TokenStream;

use quote::quote;

type Error = Box<dyn std::error::Error>;

type Result<T> = std::result::Result<T, Error>;

mod database;
mod derives;
mod query;
mod runtime;

fn macro_result(tokens: proc_macro2::TokenStream) -> TokenStream {
    quote!(
        macro_rules! macro_result {
            ($($args:tt)*) => (#tokens)
        }
    )
    .into()
}

#[proc_macro]
pub fn expand_query(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as query::QueryMacroInput);

    match query::expand_input(input) {
        Ok(ts) => ts.into(),
        Err(e) => {
            if let Some(parse_err) = e.downcast_ref::<syn::Error>() {
                macro_result(parse_err.to_compile_error())
            } else {
                let msg = e.to_string();
                macro_result(quote!(compile_error!(#msg)))
            }
        }
    }
}

#[proc_macro_derive(Encode, attributes(sqlx))]
pub fn derive_encode(tokenstream: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(tokenstream as syn::DeriveInput);
    match derives::expand_derive_encode(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(Decode, attributes(sqlx))]
pub fn derive_decode(tokenstream: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(tokenstream as syn::DeriveInput);
    match derives::expand_derive_decode(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(Type, attributes(sqlx))]
pub fn derive_type(tokenstream: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(tokenstream as syn::DeriveInput);
    match derives::expand_derive_type_encode_decode(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro_derive(FromRow, attributes(sqlx))]
pub fn derive_from_row(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);

    match derives::expand_derive_from_row(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[doc(hidden)]
#[proc_macro_attribute]
pub fn test(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemFn);

    let ret = &input.sig.output;
    let name = &input.sig.ident;
    let body = &input.block;
    let attrs = &input.attrs;

    let result = if cfg!(feature = "runtime-tokio") {
        quote! {
            #[test]
            #(#attrs)*
            fn #name() #ret {
                sqlx_rt::tokio::runtime::Builder::new()
                    .threaded_scheduler()
                    .enable_io()
                    .enable_time()
                    .build()
                    .unwrap()
                    .block_on(async { #body })
            }
        }
    } else if cfg!(feature = "runtime-async-std") {
        quote! {
            #[test]
            #(#attrs)*
            fn #name() #ret {
                sqlx_rt::async_std::task::block_on(async { #body })
            }
        }
    } else if cfg!(feature = "runtime-actix") {
        quote! {
            #[test]
            #(#attrs)*
            fn #name() #ret {
                sqlx_rt::actix_rt::System::new("sqlx-test")
                    .block_on(async { #body })
            }
        }
    } else {
        panic!("one of 'runtime-actix', 'runtime-async-std' or 'runtime-tokio' features must be enabled");
    };

    result.into()
}
