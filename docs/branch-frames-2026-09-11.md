# 完整源码帧中的分支与循环出口

436／438 使用原有 SourceFrame 事务承接已恢复的控制结构，不在 AST 或 Generate
追加跳转替代物。原始条件、帧槽与求值顺序仍须由整份函数证明。

## 当前循环的 break

`statements/breaks.rs` 消费当前 numeric/generic loop owner。循环协议匹配后，进入
正文前保存 `(LoopPlanId, exit_pc)`，离开时恢复外层上下文。正文末尾 Jump 只有在
目标等于该退出点、CFG 唯一后继吻合、冻结 `break_edges` 包含该边且 EdgeTransfer
指向同一 loop region 时，才恢复成 Break。额外 cleanup、forward route 和 iteration
动作仍拒绝。分支分析不得把该 Jump 当成普通 if/else 的 merge。

`regress_436_nested_source_frame_breaks.lua` 同时覆盖外层 generic 与内层 numeric
循环；另一变体将内层也替换成 generic。两份源码各在 stripped/debug、三种命名下
比较运行输出、GC、异常、完整 builder LIR／常量／frame，共 12 组通过。
四个字节码变体检查跨外层退出不取得当前 break 证书；跳到 latch 若可独立恢复为
合法条件臂，其完整指令也必须保持，不能误改为 break。

## 共享父分支出口

Lua 编译器可把内部条件的跳转直接连接到父 then 臂的 merge，略过父臂末尾的
中转 Jump。此时子条件的原始目标在当前解析范围外，但含义仍是正常结束该臂。

`statements/arm_exits.rs` 只在已确认的 then 末 Jump 上建立 `(arm_end, continuation)`
证书；ordinary、AND 和 OR 分支共同使用。内部目标超出当前 end 时，必须精确匹配
当前臂的 continuation，才可解释为 arm_end。新的内臂覆盖上下文，退出后恢复；
普通未知远跳、循环 break、未解释指令不会因此被省略。

`regress_438_shared_arm_continuation.lua` 覆盖 AND、嵌套 if/else 提前返回、外层
elseif 和末尾共同后缀。32 组真假组合、每组 6 个异常设置，在 6 种 stripped/debug
与命名配置下比较运行轨迹，并要求 SourceFrame 证书、完整 builder LIR／常量／frame
一致。额外 OR 变体和含兄弟 if／数值循环／臂后条件的变体也通过，共 18 组完整
函数对照。检查通过；没有执行游戏 Lua。

其它同轮结构包括 434 固定多返回局部组、435 AND 和 437 比较结果布尔物化，分别
有独立源码与全指令回归。全归档最终结果以 [总记录](all-failures-2026-09-11.md) 为准。
