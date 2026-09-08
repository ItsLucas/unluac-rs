//! branch-control 的终态共享路由、elseif transfer 与词法 join 回归。
//! 所有夹具均为原创源码，覆盖副作用、短路顺序、多返回与循环 continuation 边界。

use super::*;

pub(super) const REGRESSION_CASES_336_340: &[LuaCaseMatrixEntry] = &[
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_336_shared_terminal_predicates.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_337_elseif_call_result_join.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_338_nested_join_edge_copies.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_339_terminal_route_effect_order.lua",
        PUC_LUA_51,
    ),
    LuaCaseMatrixEntry::new(
        "tests/regress-case/regress_340_nested_loop_join_order.lua",
        PUC_LUA_51,
    ),
];
