use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use venial::{Error, FnParam, FnTypedParam, Function, Punctuated};

pub(crate) struct FnBuilder<'a> {
    orig: &'a Function,
    pub use_jump_table: bool,
    outer_params: TokenStream,
    pub param_idents: TokenStream,
    param_tys: TokenStream,
    inner_return_ty: TokenStream,
    outer_return_ty: TokenStream,
    pub inner_unsafe: Option<Ident>,
}

impl<'a> FnBuilder<'a> {
    pub fn new(orig: &'a Function) -> Result<Self, Error> {
        let mut use_jump_table = orig.qualifiers.tk_async.is_some();
        let mut outer_params = Punctuated {
            inner: vec![],
            skip_last: true,
        };
        let mut param_idents = Punctuated {
            inner: vec![],
            skip_last: true,
        };
        let mut param_tys = Punctuated {
            inner: vec![],
            skip_last: true,
        };

        for (param, _) in orig.params.iter() {
            match param {
                FnParam::Receiver(rec_param) => {
                    return Err(Error::new_at_span(
                        rec_param.tk_self.span(),
                        "make_special cannot take fn items that use self, please read the crate documentation for more details.",
                    ));
                }
                FnParam::Typed(FnTypedParam {
                    attributes,
                    name,
                    tk_colon,
                    ty,
                    ..
                }) => {
                    if ty.tokens.iter().any(|token| {
                        if let TokenTree::Ident(ident) = token {
                            ident.to_string() == "impl"
                        } else {
                            false
                        }
                    }) {
                        use_jump_table = true;
                    }

                    outer_params.push(
                        FnTypedParam {
                            attributes: attributes.clone(),
                            tk_mut: None,
                            name: name.clone(),
                            tk_colon: tk_colon.clone(),
                            ty: ty.clone(),
                        },
                        None,
                    );
                    param_idents.push(name, None);
                    param_tys.push(ty, None);
                }
            }
        }

        if let Some(generics) = &orig.generic_params {
            for (generic, _) in generics.params.iter() {
                if !generic.tk_prefix.as_ref().is_some_and(|tk_prefix| {
                    if let TokenTree::Punct(_) = tk_prefix {
                        true
                    } else {
                        false
                    }
                }) {
                    use_jump_table = true;
                    break;
                }
            }
        }

        let inner_return_ty = orig
            .return_ty
            .as_ref()
            .map(|return_ty| return_ty.to_token_stream())
            .unwrap_or(quote! {()});

        let outer_return_ty = if orig.qualifiers.tk_async.is_some() {
            quote! { impl ::core::future::Future<Output = #inner_return_ty> }
        } else {
            inner_return_ty.clone()
        };

        Ok(Self {
            orig,
            use_jump_table,
            outer_params: outer_params.into_token_stream(),
            param_idents: param_idents.into_token_stream(),
            param_tys: param_tys.into_token_stream(),
            inner_return_ty,
            outer_return_ty,
            inner_unsafe: if use_jump_table {
                orig.qualifiers.tk_unsafe.clone()
            } else {
                Some(Ident::new("unsafe", Span::call_site()))
            },
        })
    }

    fn build(
        &self,
        attributes: &[TokenStream],
        copy_async: bool,
        tk_unsafe: Option<&Ident>,
        copy_const: bool,
        name: &Ident,
        params: &TokenStream,
        return_ty: &TokenStream,
        body: TokenStream,
    ) -> TokenStream {
        let tk_async = if copy_async {
            &self.orig.qualifiers.tk_async
        } else {
            &None
        };

        let tk_const = if copy_const {
            &self.orig.qualifiers.tk_const
        } else {
            &None
        };

        let tk_extern = &self.orig.qualifiers.tk_extern;
        let extern_abi = &self.orig.qualifiers.extern_abi;
        let generics = &self.orig.generic_params;
        let where_clause = &self.orig.where_clause;

        quote! {
            #(#[#attributes])*
            #tk_const #tk_async #tk_unsafe #tk_extern #extern_abi
            fn #name #generics (#params) -> #return_ty #where_clause { #body }
        }
    }

    pub fn build_detail(
        &self,
        attributes: &[TokenStream],
        tk_unsafe: Option<&Ident>,
        copy_const: bool,
        name: &Ident,
        body: TokenStream,
    ) -> TokenStream {
        self.build(
            attributes,
            false, //copy_async
            tk_unsafe,
            copy_const,
            name,
            &self.outer_params,
            &self.outer_return_ty,
            body,
        )
    }

    pub fn build_generic(&self) -> TokenStream {
        self.build(
            &[quote!(inline(always))],
            true, //copy_async
            self.inner_unsafe.as_ref(),
            true, //copy_const
            &Ident::new("_generic", Span::call_site()),
            &self.orig.params.to_token_stream(),
            &self.inner_return_ty,
            match &self.orig.body {
                Some(body) => body.stream(),
                None => Error::new("make_special cannot take fn items without a body")
                    .to_compile_error(),
            },
        )
    }

    pub fn build_ptr(&self) -> TokenStream {
        let tk_unsafe = &self.inner_unsafe;
        let tk_extern = &self.orig.qualifiers.tk_extern;
        let extern_abi = &self.orig.qualifiers.extern_abi;
        let param_tys = &self.param_tys;
        let return_ty = &self.outer_return_ty;
        let lifetimes = self
            .orig
            .generic_params
            .iter()
            .map(|generics| {
                generics
                    .params
                    .iter()
                    .filter(|(param, _)| param.is_lifetime())
                    .map(|(param, _)| param.into_token_stream())
            })
            .flatten();

        quote! { for<#(#lifetimes),*> #tk_unsafe #tk_extern #extern_abi fn(#param_tys) -> #return_ty }
    }
}
