//! Masking secrets in text that leaves the computer (support bundles). The rules and their tests
//! live in [`pl_mc::mask`], the launcher's only masking code; this module re-exports them so the
//! bundle code and callers keep their paths.

pub use pl_mc::mask::{MASK, mask_secrets};
