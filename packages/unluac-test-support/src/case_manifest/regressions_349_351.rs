//! Complete raw nil batches must be restored before local promotion.
use super::*;

pub(super) const REGRESSION_CASES_349_351: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new("tests/regress-case/regress_349_nil_batch.lua", PUC_LUA_51),
    LuaCaseMatrixEntry::new("tests/regress-case/regress_349_nil_batch.lua", PUC_LUA_51)
        .with_variants(&[LuaCaseVariant::NamingDebugLike])
        .with_options(LuaCaseOptions {
            retain_debug: true,
            ..LuaCaseOptions::DEFAULT
        }),
    LuaCaseMatrixEntry::new("tests/regress-case/regress_350_nil_batch.lua", PUC_LUA_51),
    LuaCaseMatrixEntry::new("tests/regress-case/regress_350_nil_batch.lua", PUC_LUA_51)
        .with_variants(&[LuaCaseVariant::NamingDebugLike])
        .with_options(LuaCaseOptions {
            retain_debug: true,
            ..LuaCaseOptions::DEFAULT
        }),
    LuaCaseMatrixEntry::new("tests/regress-case/regress_351_nil_batch.lua", PUC_LUA_51),
    LuaCaseMatrixEntry::new("tests/regress-case/regress_351_nil_batch.lua", PUC_LUA_51)
        .with_variants(&[LuaCaseVariant::NamingDebugLike])
        .with_options(LuaCaseOptions {
            retain_debug: true,
            ..LuaCaseOptions::DEFAULT
        }),
];
