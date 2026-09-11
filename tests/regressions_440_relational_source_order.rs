//! 比较源码求值方向回归入口：只执行原创 Lua，并对完整 builder 做指令与帧对照。
//! 例如 `call() > field` 必须保留 CALL 在先，同时保持常量首次入池次序。

#[path = "support/lua51_roundtrip.rs"]
mod lua51_roundtrip;
#[path = "support/relational_source_order.rs"]
mod relational_source_order;
