//! Closed global array prefixes and scalar record-field calls.
use super::*;

pub(super) const REGRESSION_CASES_417: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_417_module_record_calls.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_417_module_record_calls.lua",
        PUC_LUA_51,
    )
    .with_variants(&[LuaCaseVariant::NamingDebugLike])
    .with_options(LuaCaseOptions {
        retain_debug: true,
        ..LuaCaseOptions::DEFAULT
    }),
];
