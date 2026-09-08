//! Canonical record-only children and their physical lookup scratch roots.
use super::*;

pub(super) const REGRESSION_CASES_398_399: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_398_record_lookup_regions.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_398_record_lookup_regions.lua",
        PUC_LUA_51,
    )
    .with_variants(&[LuaCaseVariant::NamingDebugLike])
    .with_options(LuaCaseOptions {
        retain_debug: true,
        ..LuaCaseOptions::DEFAULT
    }),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_399_record_lookup_gc.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_399_record_lookup_gc.lua",
        PUC_LUA_51,
    )
    .with_variants(&[LuaCaseVariant::NamingDebugLike])
    .with_options(LuaCaseOptions {
        retain_debug: true,
        ..LuaCaseOptions::DEFAULT
    }),
];
