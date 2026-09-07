use super::*;

pub(super) const REGRESSION_CASES_326_330: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_326_stale_transfer_requirement.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_327_nested_table_before_compaction.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_328_captured_table_nil_batch.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_329_nested_branch_escape.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_330_alternative_guard_routes.lua",
        PUC_LUA_51,
    ),
];
