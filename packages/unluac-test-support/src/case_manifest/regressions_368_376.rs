//! Rounded allocation hints must not split the final pure record child into a local root.
use super::*;

const fn debug(entry: LuaCaseMatrixEntry) -> LuaCaseMatrixEntry {
    entry
        .with_variants(&[LuaCaseVariant::NamingDebugLike])
        .with_options(LuaCaseOptions {
            retain_debug: true,
            ..LuaCaseOptions::DEFAULT
        })
}

pub(super) const REGRESSION_CASES_368_376: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_368_rounded_records_17.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_368_rounded_records_17.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_369_rounded_records_18.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_369_rounded_records_18.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_370_rounded_records_31.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_370_rounded_records_31.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_371_rounded_records_32.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_371_rounded_records_32.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_372_rounded_records_33.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_372_rounded_records_33.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_373_rounded_records_34.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_373_rounded_records_34.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_374_rounded_record_explicit.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_374_rounded_record_explicit.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_375_rounded_record_mutation.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_375_rounded_record_mutation.lua",
        PUC_LUA_51,
    )),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_376_rounded_named_child.lua",
        PUC_LUA_51,
    ),
    debug(LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_376_rounded_named_child.lua",
        PUC_LUA_51,
    )),
];
