//! Tests for `#[serde(skip_serializing_if = ...)]`, so empty fields stay out of the
//! saved files.

/// Whether a number is zero.
pub fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// Whether a whole number is zero.
pub fn is_zero_i(v: &i64) -> bool {
    *v == 0
}

/// Whether a flag is off.
pub fn is_false(b: &bool) -> bool {
    !*b
}
