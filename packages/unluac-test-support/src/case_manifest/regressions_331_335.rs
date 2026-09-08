//! 构造器回归覆盖嵌套快照与 open 前缀退役；slot 复用、调用和 nil 数组若缺少
//! 完整布局/物理 root 证明则要求 typed residual。全部使用原创源码和官方 toolchain。

use super::*;

pub(super) const REGRESSION_CASES_331_335: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_331_reused_mixed_constructor.lua",
        PUC_LUA_51,
    )
    .with_expectation(LuaCaseExpectation::TableSetListResidual),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_332_reused_nested_constructor.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_333_reused_constructor_call_order.lua",
        PUC_LUA_51,
    )
    .with_expectation(LuaCaseExpectation::TableSetListResidual),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_334_open_constructor_retired_prefix.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_335_preallocated_nil_batch.lua",
        PUC_LUA_51,
    )
    .with_expectation(LuaCaseExpectation::TableSetListResidual),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_335_preallocated_nil_batch.lua",
        PUC_LUA_51,
    )
    .with_variants(&[LuaCaseVariant::NamingDebugLike])
    .with_options(LuaCaseOptions {
        retain_debug: true,
        ..LuaCaseOptions::DEFAULT
    })
    .with_expectation(LuaCaseExpectation::TableSetListResidual),
];
