//! Shared-tail fence boundaries before and after condition folding.
use super::*;

const fn debug(entry: LuaCaseMatrixEntry) -> LuaCaseMatrixEntry {
    entry
        .with_variants(&[LuaCaseVariant::NamingDebugLike])
        .with_options(LuaCaseOptions {
            retain_debug: true,
            ..LuaCaseOptions::DEFAULT
        })
}

pub(super) const REGRESSION_CASES_396: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_396_nested_shared_tail.lua",
        PUC_LUA_ALL,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_396_nested_shared_tail.lua",
        PUC_LUA_ALL,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_396_folded_fence_guard.lua",
        PUC_LUA_ALL,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_396_folded_fence_guard.lua",
        PUC_LUA_ALL,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_396_shared_guard_terminal.lua",
        PUC_LUA_ALL,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_396_shared_guard_terminal.lua",
        PUC_LUA_ALL,
    )),
];
