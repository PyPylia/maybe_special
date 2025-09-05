use crate::{Architecture, FnBuilder};
use proc_macro2::{Ident, Literal, Span, TokenStream, TokenTree};
use quote::{ToTokens, quote};
use std::collections::{HashMap, HashSet};
use venial::Error;

macro_rules! expect_token {
    ($token:ident = $expr:expr, $msg:literal) => {
        match $expr {
            Some(TokenTree::$token(token)) => token,
            Some(other) => {
                return Err(Error::new_at_span(
                    other.span(),
                    format!("expected {} but got {}", $msg, other),
                ))
            }
            None => return Err(Error::new(format!("expected {} but found nothing", $msg))),
        }
    };
}

pub struct Specialisation<'a> {
    builder: &'a FnBuilder<'a>,
    pub arch: Architecture,
    pub features: HashSet<String>,
    pub is_static: bool,
    pub is_manual: bool,
    pub ident: Ident,
}

impl<'a> Specialisation<'a> {
    pub(crate) fn parse(
        builder: &'a FnBuilder<'a>,
        attr: TokenStream,
    ) -> Result<HashMap<Architecture, Vec<Self>>, Error> {
        let mut output = HashMap::new();
        let mut iter = attr.into_iter();

        while let Some(TokenTree::Ident(arch_ident)) = iter.next() {
            let is_static;
            let arch: Architecture = match arch_ident.to_string() {
                val if val == "static" => {
                    is_static = true;
                    expect_token!(Ident = iter.next(), "an architecture").to_string()
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

            let arch_str = arch.as_str();
            let mut name = String::with_capacity(1 + arch_str.len());
            name.push('_');
            name.push_str(arch_str);

            let _equals = iter
                .next()
                .ok_or_else(|| Error::new("expected = but found nothing"))?;

            let features = parse_features(&mut iter, &mut name)?;
            let is_manual;
            let ident = match parse_ident(&mut iter)? {
                Some(ident) => {
                    is_manual = true;
                    ident
                }
                None => {
                    is_manual = false;
                    Ident::new(&name, Span::call_site())
                }
            };

            output
                .entry(arch)
                .or_insert_with(|| vec![])
                .push(Specialisation {
                    builder,
                    arch,
                    features,
                    is_static,
                    is_manual,
                    ident,
                });
        }

        Ok(output)
    }
}

fn parse_features(
    iter: &mut impl Iterator<Item = TokenTree>,
    name: &mut String,
) -> Result<HashSet<String>, Error> {
    let mut features = HashSet::new();
    let mut iter = expect_token!(Group = iter.next(), "[\"feature\", \"feature\", ...]")
        .stream()
        .into_iter();

    while let Some(TokenTree::Literal(lit)) = iter.next() {
        if let litrs::Literal::String(inner) = lit.clone().into() {
            let feature = inner.into_value();

            name.reserve(feature.len() + 1);
            name.push('_');
            for ch in feature.chars() {
                if unicode_ident::is_xid_continue(ch) {
                    name.push(ch);
                }
            }

            features.insert(feature);
        } else {
            return Err(Error::new_at_span(
                lit.span(),
                format!("expected a string literal but got {}", lit),
            ));
        }
    }

    if features.is_empty() {
        Err(Error::new("expected features but found nothing"))
    } else {
        Ok(features)
    }
}

fn parse_ident(iter: &mut impl Iterator<Item = TokenTree>) -> Result<Option<Ident>, Error> {
    let mut ident = None;
    if let Some(TokenTree::Punct(punct)) = iter.next() {
        if punct.as_char() == ',' {
            return Ok(None);
        }

        let _gt = iter.next();
        let unsafe_ident = expect_token!(Ident = iter.next(), "unsafe");
        let unsafe_str = unsafe_ident.to_string();
        if unsafe_str == "unsafe" {
            ident = Some(expect_token!(Ident = iter.next(), "ident"));
        } else {
            return Err(Error::new_at_span(
                unsafe_ident.span(),
                "manual impls must be prefixed with unsafe",
            ));
        }

        let _comma = iter.next();
    }

    Ok(ident)
}

impl ToTokens for Specialisation<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        if self.is_manual {
            return;
        }

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
            &self.ident,
            quote! { #inner_unsafe { _generic(#param_idents) } },
        ));
    }
}
