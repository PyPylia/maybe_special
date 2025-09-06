use crate::{FnBuilder, Specialisation, generic_ident};
use indexmap::IndexSet;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;
use venial::Function;

pub fn make_special(attr: TokenStream, orig_func: Function) -> TokenStream {
    let builder = match FnBuilder::new(&orig_func) {
        Ok(builder) => builder,
        Err(err) => return err.to_compile_error().into(),
    };

    let specialisations = match Specialisation::parse(&builder, attr) {
        Ok(specs) => specs,
        Err(err) => return err.to_compile_error().into(),
    };

    let generic_call = builder.build_call(&generic_ident());
    let param_idents = &builder.param_idents;
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
            (
                quote! { ::core::sync::atomic::AtomicUsize },
                quote! { ::core::sync::atomic::AtomicUsize::new(0) },
            )
        } else {
            (
                quote! { ::core::sync::atomic::AtomicPtr<()> },
                quote! { ::core::sync::atomic::AtomicPtr::new(#init_ident as *mut ()) },
            )
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
            |(i, _)| quote! { #i + 2 }
        } else {
            |(_, spec): (usize, &Specialisation)| {
                let spec_ident = &spec.ident;
                quote! { #spec_ident as *mut () }
            }
        });

        let prefix = if cfg!(feature = "std") {
            quote! { ::std::arch:: }
        } else {
            quote! { ::std_detect:: }
        };

        let generic_val = if builder.use_jump_table {
            quote! { 1 }
        } else {
            quote! { _generic as *mut () }
        };

        let dispatch_call = builder.build_call(&dispatch_ident);
        init.push(builder.build_detail(
            &[quote!(cfg(#cfg_inner))],
            false, //copy_const
            true,  //copy_unsafe
            &init_ident,
            quote! {
                unsafe {
                    #jump_ref_ident.store(
                        match (#(#prefix #detect_macro !(#feature_literal)),*) {
                            #(#spec_criteria => #spec_val,)*
                            _ => #generic_val
                        },
                        ::core::sync::atomic::Ordering::Relaxed
                    );
                }
                #dispatch_call
            },
        ));

        // DISPATCH

        let static_call = specs.iter().filter(|spec| spec.is_static).map(|spec| {
            let feature = spec.features.iter().map(|feature| Literal::string(feature));
            quote! {
                #[cfg(all(#(target_feature = #feature),*))]
                return #generic_call;
            }
        });

        let dyn_call = if builder.use_jump_table {
            let init_call = builder.build_call(&init_ident);
            let spec_index = 2..=specs.len() + 2;
            let spec_call = specs.iter().map(|spec| builder.build_call(&spec.ident));

            quote! {
                match unsafe { #jump_ref_ident.load(::core::sync::atomic::Ordering::Relaxed) } {
                    0 => #init_call,
                    1 => #generic_call,
                    #(
                        #spec_index => unsafe #spec_call,
                    )*
                    _ => unsafe { ::core::hint::unreachable_unchecked() }
                }
            }
        } else {
            let fn_ptr = builder.build_ptr();
            quote! {
                unsafe {
                    ::core::mem::transmute::<*mut (), #fn_ptr>(
                        #jump_ref_ident.load(::core::sync::atomic::Ordering::Relaxed)
                    )(#param_idents)
                }
            }
        };

        dispatch.push(builder.build_detail(
            &[
                quote!(cfg(#cfg_inner)),
                quote!(allow(unreachable_code)),
                quote!(inline(always)),
            ],
            false, //copy_const
            false, //copy_unsafe
            &dispatch_ident,
            quote! {
                #[cfg(all(#(target_feature = #feature_literal),*))]
                return #generic_call;

                #(#static_call)*
                #dyn_call
            },
        ));

        // ARCH CALL

        arch_call.push(if orig_func.qualifiers.tk_const.is_some() {
            let safe_generic = builder.build_detail(
                &[quote!(cfg(#cfg_inner)), quote!(inline(always))],
                true, //copy_const
                false, //copy_unsafe
                &Ident::new("_safe_generic", Span::call_site()),
                generic_call.clone(),
            );

            quote! {
                #safe_generic

                #[cfg(#cfg_inner)]
                return ::core::intrinsics::const_eval_select((#param_idents), _safe_generic, #dispatch_ident);
            }
        } else {
            quote! {
                #[cfg(#cfg_inner)]
                return #dispatch_call;
            }
        });
    }

    let attributes = &orig_func.attributes;
    let vis_marker = &orig_func.vis_marker;
    let outer_def = builder.build_detail(
        &[],  //attributes
        true, //copy_const
        true, //copy_unsafe
        &orig_func.name,
        quote! {
            #generic
            #(#spec)*
            #(#jump_ref)*
            #(#init)*
            #(#dispatch)*
            #(#arch_call)*
            #[allow(unreachable_code)]
            #generic_call
        },
    );

    quote! {
        #(#attributes)* #vis_marker #outer_def
    }
}
