# Rust Dynamic Target Feature Specialisation Macro

[<img alt="github" src="https://img.shields.io/badge/github-dtolnay/quote-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/pypylia/maybe_special)
[<img alt="crates.io" src="https://img.shields.io/crates/v/quote.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/maybe_special)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-quote-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/maybe_special)

This crate provides the [`#[make_special]`] attribute macro to create a series
of target feature specialisations for the given function.

```toml
[dependencies]
maybe_special = "1.0"
```

_This crate is designed for Rust edition 2024 (rustc 1.85+)._

# Usage

This macro takes in a series of specialisations in the form `arch =
["feature1", "feature2", ...]`. This macro uses [`std::arch`]/[`std_detect`]
under the hood, so look at their documentation for more details, especially
since some architectures are currently unstable. Additionally,
specialisations can be marked with `static` to enable static dispatch on
them, which is explained below.

**Note: This macro does not figure out which specialisations are most
optimal for you, that is still something you must benchmark yourself. Rather
this macro merely aids in writing specialisations, and handling their dispatch.**

### Example

```rs
#[maybe_special::make_special(
    x86 = ["avx512f", "avx512vl"],
    static x86 = ["sse4"],
    riscv = ["v"]
)]
pub fn fast_dot_product(a: [u8; 16], b: [u8; 16]) -> usize {
    a.iter().zip(b.iter()).map(|(a, b)| (a * b) as usize).sum()
}
```

# Use on types that use `self`/`Self`

To allow this macro to work anywhere it must generate the specialisations
inside the outer function, however this has the side-effect of not working
for types that use `self`/`Self` (because the inner function doesn't know
what `Self` is).

To get around this, you can do something like the following:

```rs
impl SomeType {
    fn clone_multiple(&self, num: usize) -> Vec<Self> {
        #[maybe_special::make_special(x86 = ["avx2"])]
        #[inline(always)]
        fn inner(val: &SomeType, num: usize) -> Vec<SomeType> {
            vec![val.clone(); num]
        }

        inner(self, num)
    }
}
```

# `no_std` support

By default, this macro utilises [`std::arch`], however this can be disabled
by disabling the `std` feature. When the `std` feature is disabled, the code
generated will instead use the unstable [`std_detect`] module, which must be
included manually.

# Dispatch types

When calling the outer function, this macro utilises a dispatch function to
figure out which specialisation to use. The different dispatch methods are
documented below.

### Const dispatch

When applied to a `const fn`, this macro utilises the [`const_eval_select`]
compiler intrinsic to either branch to the inner impl at compile-time, or
the regular dynamic dispatch function at run-time. However, this
intrinsic is currently unstable, so you will need to add
`#![feature(core_intrinsics, const_eval_select)]` to your crate to use this.

### Static dispatch

When the executable/library is being compiled with all checked features
enabled, this macro will skip dynamic dispatch, and jump directly to the
inner impl. You can also manually mark a specialisation to do this even if
features not specified are not enabled with the `static` keyword. This macro
will pick the first static-dispatchable specialisation that meets all its
criteria (or use dynamic dispatch if none meet their criteria at
compile-time).

### Function pointer dispatch

This is the default dispatch method. This macro generates a static mutable
function pointer that is called upon calling the outer function. Upon first
call, instead of directly calling a specialisation or the generic impl, it
instead calls an initialiser function that checks for all enabled features
at run-time, and determines the best specialisation to call. This result is
saved so that all future calls are fast.

### Jump table dispatch

When applied to a function that contains generics, `impl` types, or is
`async`, function pointer dispatch will not work. This is because all types
must be specified exactly to generate a function pointer. `async` functions
under the hood desugar to returning an `impl Future<Output = Ty>`,
therefore making them also behave as if they were generic. Therefore, this
macro falls back to a jump table dispatch method, where instead of utilising
a function pointer directly, it instead utilises an index into a jump table.
This dispatch method is almost identical to the function pointer method,
however can be a few cycles slower.

[`#[make_special]`]: https://docs.rs/maybe_special/1.0/maybe_special/macro.maybe_special.html

[`std::arch`]: https://doc.rust-lang.org/stable/std/arch/index.html
[`std_detect`]: https://doc.rust-lang.org/nightly/std_detect/index.html
[`const_eval_select`]: https://doc.rust-lang.org/stable/core/intrinsics/fn.const_eval_select.html
