//! Captured constructor roots, stack arithmetic, and canonical open tails.
use super::*;

pub(super) const REGRESSION_CASES_413_414: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_413_captured_record_array.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_413_captured_record_array.lua",
        PUC_LUA_51,
    )
    .with_variants(&[LuaCaseVariant::NamingDebugLike])
    .with_options(LuaCaseOptions {
        retain_debug: true,
        ..LuaCaseOptions::DEFAULT
    }),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_414_captured_open_tail.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_414_captured_open_tail.lua",
        PUC_LUA_51,
    )
    .with_variants(&[LuaCaseVariant::NamingDebugLike])
    .with_options(LuaCaseOptions {
        retain_debug: true,
        ..LuaCaseOptions::DEFAULT
    }),
];
