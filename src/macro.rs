use crate::{FnBuilder, Specialisation};
use indexmap::IndexSet;
use proc_macro2::{Ident, Literal, Span, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use venial::Function;

pub fn make_special(attr: TokenStream, orig_func: Function) -> TokenStream {
    let builder = match FnBuilder::new(&orig_func) {
        Ok(builder) => builder,
        Err(err) => return err.to_compile_error().into(),
    };

    let inner_unsafe = &builder.inner_unsafe;
    let param_idents = &builder.param_idents;
    let specialisations = match Specialisation::parse(&builder, attr) {
        Ok(specs) => specs,
        Err(err) => return err.to_compile_error().into(),
    };

    let generic = builder.build_generic();
    let spec = specialisations.values().flatten();
    let mut jump_ref = Vec::with_capacity(specialisations.len());
    let mut init = Vec::with_capacity(specialisations.len());
    let mut dispatch = Vec::with_capacity(specialisations.len());
    let mut arch_call = Vec::with_capacity(specialisations.len());

    for (arch, specs) in &specialisations {
        let cfg_inner = arch.cfg_inner();
        let dispatch_ident = arch.dispatch_ident();
        let jump_ref_ident = arch.jump_ref_ident();
        let init_ident = arch.init_ident();
        let detect_macro = arch.detect_macro();

        let features: IndexSet<String> = specs
            .iter()
            .map(|spec| spec.features.clone())
            .flatten()
            .collect();

        let feature_literal: Vec<Literal> = features
            .iter()
            .map(|feature| Literal::string(&feature))
            .collect();

        // JUMP REF

        let (jump_ref_ty, jump_ref_val) = if builder.use_jump_table {
            (quote! { usize }, quote! { 0 })
        } else {
            (builder.build_ptr(), arch.init_ident().into_token_stream())
        };

        jump_ref.push(quote! {
            #[cfg(#cfg_inner)]
            static mut #jump_ref_ident: #jump_ref_ty = #jump_ref_val;
        });

        // INIT

        let spec_criteria = specs.iter().map(|spec| {
            let feature_pat = features.iter().map(|feature| {
                if spec.features.contains(feature) {
                    quote! { true }
                } else {
                    quote! { _ }
                }
            });

            quote! {
                (#(#feature_pat),*)
            }
        });

        let spec_val = specs.iter().enumerate().map(if builder.use_jump_table {
            |(i, _)| TokenTree::Literal(Literal::usize_suffixed(i + 2))
        } else {
            |(_, spec): (usize, &Specialisation)| TokenTree::Ident(spec.as_ident())
        });

        let prefix = if cfg!(feature = "std") {
            quote! { ::std::arch:: }
        } else {
            quote! { ::std_detect:: }
        };

        let generic_val = if builder.use_jump_table {
            quote! { 1 }
        } else {
            quote! { _generic }
        };

        init.push(builder.build_detail(
            &[quote!(cfg(#cfg_inner))],
            builder.inner_unsafe.as_ref(),
            false, //copy_const
            &init_ident,
            quote! {
                unsafe {
                    #jump_ref_ident = match (#(#prefix #detect_macro !(#feature_literal)),*) {
                        #(#spec_criteria => #spec_val,)*
                        _ => #generic_val
                    };
                }
                #dispatch_ident(#param_idents)
            },
        ));

        // DISPATCH

        let static_call = specs.iter().filter(|spec| spec.is_static).map(|spec| {
            let feature = spec.features.iter().map(|feature| Literal::string(feature));
            quote! {
                #[cfg(all(#(target_feature = #feature),*))]
                return #inner_unsafe { _generic(#param_idents) };
            }
        });

        let dyn_call = if builder.use_jump_table {
            let init_ident = arch.init_ident();
            let spec_index = 2..=specs.len() + 2;
            let spec_ident = specs.iter().map(|spec| spec.as_ident());

            quote! {
                match unsafe { #jump_ref_ident } {
                    0 => #init_ident(#param_idents),
                    1 => _generic(#param_idents),
                    #(
                        #spec_index => unsafe { #spec_ident(#param_idents) },
                    )*
                    _ => unsafe { ::core::hint::unreachable_unchecked() }
                }
            }
        } else {
            quote! {
                unsafe { #jump_ref_ident(#param_idents) }
            }
        };

        dispatch.push(builder.build_detail(
            &[
                quote!(cfg(#cfg_inner)),
                quote!(allow(unreachable_code)),
                quote!(inline(always)),
            ],
            None,  //tk_unsafe
            false, //copy_const
            &arch.dispatch_ident(),
            quote! {
                #[cfg(all(#(target_feature = #feature_literal),*))]
                return #inner_unsafe { _generic(#param_idents) };

                #(#static_call)*
                #dyn_call
            },
        ));

        // ARCH CALL

        arch_call.push(if orig_func.qualifiers.tk_const.is_some() {
            let safe_generic = builder.build_detail(
                &[quote!(cfg(#cfg_inner)), quote!(inline(always))],
                None, //tk_unsafe
                true, //copy_const
                &Ident::new("_safe_generic", Span::call_site()),
                quote! {
                    #inner_unsafe { _generic(#param_idents) }
                },
            );

            quote! {
                #safe_generic

                #[cfg(#cfg_inner)]
                return ::core::intrinsics::const_eval_select((#param_idents), _safe_generic, #dispatch_ident);
            }
        } else {
            quote! {
                #[cfg(#cfg_inner)]
                return #dispatch_ident(#param_idents);
            }
        });
    }

    let attributes = &orig_func.attributes;
    let vis_marker = &orig_func.vis_marker;
    let outer_def = builder.build_detail(
        &[], //attributes
        orig_func.qualifiers.tk_unsafe.as_ref(),
        true, //copy_const
        &orig_func.name,
        quote! {
            #generic
            #(#spec)*
            #(#jump_ref)*
            #(#init)*
            #(#dispatch)*
            #(#arch_call)*
            #[allow(unreachable_code)]
            #inner_unsafe { _generic(#param_idents) }
        },
    );

    quote! {
        #(#attributes)* #vis_marker #outer_def
    }
}
