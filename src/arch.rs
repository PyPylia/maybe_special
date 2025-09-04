use proc_macro2::{Ident, Literal, TokenStream, TokenTree};
use quote::{ToTokens, TokenStreamExt, format_ident, quote};
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Architecture {
    AARCH64,
    LOONGARCH,
    RISCV,
    X86,
    ARM,
    MIPS64,
    MIPS32,
    POWERPC64,
    POWERPC32,
    S390X,
}

impl Architecture {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AARCH64 => "aarch64",
            Self::LOONGARCH => "loongarch",
            Self::RISCV => "riscv",
            Self::X86 => "x86",
            Self::ARM => "arm",
            Self::MIPS64 => "mips64",
            Self::MIPS32 => "mips",
            Self::POWERPC64 => "powerpc64",
            Self::POWERPC32 => "powerpc",
            Self::S390X => "s390x",
        }
    }

    pub fn cfg_inner(&self) -> TokenStream {
        match self {
            Self::X86 => quote! { any(target_arch = "x86", target_arch = "x86_64") },
            Self::RISCV => quote! { any(target_arch = "riscv32", target_arch = "riscv64") },
            Self::ARM => quote! { any(target_arch = "arm", target_arch = "arm64ec") },
            Self::MIPS32 => quote! { any(target_arch = "mips", target_arch = "mips32r6") },
            Self::MIPS64 => quote! { any(target_arch = "mips64", target_arch = "mips64r6") },
            other => quote! { target_arch = #other },
        }
    }

    pub fn dispatch_ident(&self) -> Ident {
        format_ident!("_dispatch_{}", self.as_str())
    }

    pub fn jump_ref_ident(&self) -> Ident {
        format_ident!("JUMP_REF_{}", &self.as_str())
    }

    pub fn init_ident(&self) -> Ident {
        format_ident!("_init_{}", self.as_str())
    }

    pub fn detect_macro(&self) -> Ident {
        format_ident!("is_{}_feature_detected", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnimplementedArch;

impl FromStr for Architecture {
    type Err = UnimplementedArch;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "aarch64" => Architecture::AARCH64,
            "loongarch" => Architecture::LOONGARCH,
            "riscv" => Architecture::RISCV,
            "x86" => Architecture::X86,
            "x86_64" => Architecture::X86,
            "arm" => Architecture::ARM,
            "mips64" => Architecture::MIPS64,
            "mips32" => Architecture::MIPS32,
            "mips" => Architecture::MIPS32,
            "powerpc64" => Architecture::POWERPC64,
            "powerpc32" => Architecture::POWERPC32,
            "powerpc" => Architecture::POWERPC32,
            "s390x" => Architecture::S390X,
            _ => return Err(UnimplementedArch),
        })
    }
}

impl From<Architecture> for &str {
    fn from(value: Architecture) -> Self {
        value.as_str()
    }
}

impl ToTokens for Architecture {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append(TokenTree::Literal(Literal::string(self.as_str())));
    }
}
