//! In-place record arithmetic, including metamethod order and exceptions.
use super::*;

pub(super) const REGRESSION_CASES_412: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_412_record_arithmetic.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_412_record_arithmetic.lua",
        PUC_LUA_51,
    )
    .with_variants(&[LuaCaseVariant::NamingDebugLike])
    .with_options(LuaCaseOptions {
        retain_debug: true,
        ..LuaCaseOptions::DEFAULT
    }),
];
