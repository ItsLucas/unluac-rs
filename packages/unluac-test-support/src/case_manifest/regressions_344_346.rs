//! 完整嵌套字面量保留数组布局，未知旧对象的标量覆盖仍要求 typed residual。

use super::*;

pub(super) const REGRESSION_CASES_344_346: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_344_nested_literal_array_layout.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_345_scalar_constructor_root_overwrite.lua",
        PUC_LUA_51,
    )
    .with_expectation(LuaCaseExpectation::TableSetListResidual),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_346_nested_literal_batch_boundary.lua",
        PUC_LUA_51,
    ),
];
