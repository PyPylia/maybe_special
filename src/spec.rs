use crate::{Architecture, FnBuilder};
use proc_macro2::{Ident, Literal, Span, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use std::collections::{HashMap, HashSet};
use venial::Error;

pub struct Specialisation<'a> {
    builder: &'a FnBuilder<'a>,
    pub arch: Architecture,
    pub features: HashSet<String>,
    pub is_static: bool,
}

impl<'a> Specialisation<'a> {
    pub fn as_ident(&self) -> Ident {
        let arch = self.arch.as_str();
        let mut name = String::with_capacity(1 + arch.len() + self.features.len());
        name.push('_');
        name.push_str(arch);

        for feature in &self.features {
            name.reserve(feature.len());
            name.push('_');

            for char in feature.chars() {
                if unicode_ident::is_xid_continue(char) {
                    name.push(char.to_ascii_lowercase());
                }
            }
        }

        Ident::new(&name, Span::call_site())
    }

    pub(crate) fn parse(
        builder: &'a FnBuilder<'a>,
        attr: TokenStream,
    ) -> Result<HashMap<Architecture, Vec<Self>>, Error> {
        let mut output = HashMap::new();
        let mut iter = attr.into_iter();

        while let Some(TokenTree::Ident(arch_ident)) = iter.next() {
            let is_static;
            let arch = match arch_ident.to_string() {
                val if val == "static" => {
                    is_static = true;
                    match iter.next() {
                        Some(TokenTree::Ident(arch_ident)) => arch_ident.to_string(),
                        Some(other) => {
                            return Err(Error::new_at_span(
                                other.span(),
                                format!("expected an architecture but got {}", other),
                            ));
                        }
                        None => {
                            return Err(Error::new("expected an architecture but found nothing"));
                        }
                    }
                }
                other => {
                    is_static = false;
                    other
                }
            }
            .parse()
            .map_err(|_| {
                Error::new_at_span(
                    arch_ident.span(),
                    format!("{} is not a supported architecture", arch_ident),
                )
            })?;

            let _equals = iter
                .next()
                .ok_or_else(|| Error::new("expected = but found nothing"))?;

            let mut features = HashSet::new();
            match iter.next() {
                Some(TokenTree::Group(group)) => {
                    let mut iter = group.stream().into_iter();
                    while let Some(TokenTree::Literal(lit)) = iter.next() {
                        if let litrs::Literal::String(inner) = lit.clone().into() {
                            features.insert(inner.into_value().to_owned());
                        } else {
                            return Err(Error::new_at_span(
                                lit.span(),
                                format!("expected a string literal but got {}", lit),
                            ));
                        }

                        let _comma = iter.next();
                    }
                }
                Some(other) => {
                    return Err(Error::new_at_span(
                        other.span(),
                        format!("expected [\"feature\", \"feature\", ...] but got {}", other),
                    ));
                }
                None => {
                    return Err(Error::new(
                        "expected [\"feature\", \"feature\", ...] but found nothing",
                    ));
                }
            }

            if features.is_empty() {
                return Err(Error::new("expected features but found nothing"));
            }

            let _comma = iter.next();

            output
                .entry(arch)
                .or_insert_with(|| vec![])
                .push(Specialisation {
                    builder,
                    arch,
                    features,
                    is_static,
                });
        }

        Ok(output)
    }
}

impl ToTokens for Specialisation<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut features = String::new();

        for feature in &self.features {
            if !features.is_empty() {
                features.push(',');
            }

            features.reserve(feature.len() + 1);
            features.push_str(feature);
        }

        let enabled_features = Literal::string(&features);
        let cfg_inner = self.arch.cfg_inner();
        let attributes = &[
            quote!(cfg(#cfg_inner)),
            quote!(target_feature(enable = #enabled_features)),
        ];

        let inner_unsafe = self.builder.inner_unsafe.as_ref();
        let param_idents = &self.builder.param_idents;
        tokens.extend(self.builder.build_detail(
            attributes,
            inner_unsafe,
            true, //copy_const
            &self.as_ident(),
            quote! { #inner_unsafe { _generic(#param_idents) } },
        ));
    }
}
