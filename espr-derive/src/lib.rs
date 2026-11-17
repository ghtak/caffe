use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

/// 검증 규칙이 없는 타입에 성공하는 `Validate` 구현을 생성합니다.
#[proc_macro_derive(Validate)]
pub fn derive_validate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    quote! {
        impl #impl_generics ::espr::validate::Validate for #name #type_generics #where_clause {
            type Error = ::core::convert::Infallible;

            fn validate(&self) -> ::core::result::Result<(), Self::Error> {
                Ok(())
            }
        }
    }
    .into()
}
