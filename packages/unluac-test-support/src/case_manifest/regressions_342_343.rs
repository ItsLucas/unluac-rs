//! 以源码和官方 VM oracle 保护 SETLIST 的物理 root 与数组布局边界；
//! 证明不足时必须保留 typed residual，不能以可编译源码替代语义等价。

use super::*;

pub(super) const REGRESSION_CASES_342_343: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_342_constructor_previous_iteration_root.lua",
        PUC_LUA_51,
    )
    .with_expectation(LuaCaseExpectation::TableSetListResidual),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_343_mixed_constructor_array_layout.lua",
        PUC_LUA_51,
    )
    .with_expectation(LuaCaseExpectation::TableSetListResidual),
];
