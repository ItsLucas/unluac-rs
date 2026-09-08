//! Canonical effectful array regions; wire this slice into case_manifest.rs.
use super::*;

pub(super) const REGRESSION_CASES_397: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_397_effectful_array_regions.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_397_effectful_array_regions.lua",
        PUC_LUA_51,
    )
    .with_variants(&[LuaCaseVariant::NamingDebugLike])
    .with_options(LuaCaseOptions {
        retain_debug: true,
        ..LuaCaseOptions::DEFAULT
    }),
];
