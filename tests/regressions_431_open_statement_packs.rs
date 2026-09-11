//! 开放 pack 的源码回归由同一个 Lua 5.1 工具链贯通，不执行外部游戏样本。
#[path = "support/lua51_roundtrip.rs"]
mod lua51_roundtrip;

#[path = "support/open_statement_packs.rs"]
mod open_statement_packs;
