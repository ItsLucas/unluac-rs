//! Atomic once-only conditionals and literal integer record keys.
use super::*;

const fn debug(entry: LuaCaseMatrixEntry) -> LuaCaseMatrixEntry {
    entry
        .with_variants(&[LuaCaseVariant::NamingDebugLike])
        .with_options(LuaCaseOptions {
            retain_debug: true,
            ..LuaCaseOptions::DEFAULT
        })
}

pub(super) const REGRESSION_CASES_390_395: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_390_conditional_integer_matrix.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_390_conditional_integer_matrix.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_391_integer_records_and_lists.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_391_integer_records_and_lists.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_392_record_list_collision.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_392_record_list_collision.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_393_ancestor_capture.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_393_ancestor_capture.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_394_integer_hash_layout.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_394_integer_hash_layout.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_395_dynamic_record_keys.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_395_dynamic_record_keys.lua",
        PUC_LUA_51,
    )),
];
