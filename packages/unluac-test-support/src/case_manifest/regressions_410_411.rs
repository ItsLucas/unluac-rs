//! A single-pass fence tail is not an explicit continue to the VM-for control block.
use super::*;

const fn debug(entry: LuaCaseMatrixEntry) -> LuaCaseMatrixEntry {
    entry
        .with_variants(&[LuaCaseVariant::NamingDebugLike])
        .with_options(LuaCaseOptions {
            retain_debug: true,
            ..LuaCaseOptions::DEFAULT
        })
}

pub(super) const REGRESSION_CASES_410_411: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_410_for_fence_tail.lua",
        ALL_DIALECTS,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_410_for_fence_tail.lua",
        ALL_NON_LUAU_DIALECTS,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_411_nested_fence_tails.lua",
        ALL_DIALECTS,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_411_nested_fence_tails.lua",
        ALL_NON_LUAU_DIALECTS,
    )),
];
