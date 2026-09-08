//! 短路条件的单臂边界必须保留所有正常汇入点，不能吞掉 if/else 的共享尾部。

use super::*;

pub(super) const REGRESSION_CASES_341: &[LuaCaseMatrixEntry] = &[LuaCaseMatrixEntry::new(
    "tests/regress-case/regress_341_short_circuit_shared_continuation.lua",
    PUC_LUA_51,
)];
