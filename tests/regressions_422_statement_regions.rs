//! 以官方 Lua 工具链验证完整语句帧，覆盖原始槽写、运行输出与严格拒绝边界。
#[path = "support/lua51_roundtrip.rs"]
mod lua51_roundtrip;

#[path = "support/statement_regions.rs"]
mod statement_regions;
