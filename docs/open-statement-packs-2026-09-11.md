# 开放调用参数与完整源码帧

本轮只读定位端午与无名剑的四份真实输入，并在现有 statement source-frame 事务内
补齐开放 pack。真实文件仅做 Strict 生成和官方 Lua 5.1 重编译，不执行游戏代码。

| 输入 SHA-256 前缀 | 原失败位置 | 原始布局 | 当前结果 |
| --- | --- | --- | --- |
| `4ccb42199a51` | 端午·粽头戏_体型，proto#11 | 0 参数、maxstack 6；两项固定 CALL、末项开放 CALL、SETLIST、RETURN 表 | Strict + luac -p 通过 |
| `87e44ab528b4` | 同上，另一版本 proto#11 | 与前一份相同的 16 条 LIR | Strict + luac -p 通过 |
| `9d2a7aac37b9` | 无名剑二代，proto#9 | 4 参数、maxstack 12；一臂开放 CALL 作 Message 参数，另一臂开放数组局部及查表调用 | Strict + luac -p 通过 |
| `71fb3a141f93` | 端午旧版本，proto#0 | maxstack 14；未读取常量槽、LEN/SUB 捕获声明、三个连续捕获开放数组及闭包安装 | Strict + luac -p 通过 |

端午 proto#11 的关键协议是 `CALL r3 fixed(r4..r5) -> open(r3)`，紧随
`SETLIST r0 open(r1)`，其中 `r1/r2` 是前两个固定项。无名剑的消息臂是
`CALL r5 fixed(r6..r7) -> open(r5)`，紧随 `CALL r4 open(r5) -> ignore`；
数组臂为 `NEWTABLE r4` 到 `SETLIST r4 open(r5)`，随后 `r4[actor.force]` 的
动态 key 已由共享表达式栈按原 `GETTABLE r8 = r4[r8]` 的同槽覆盖处理。

旧端午不是单纯开放尾项缺口：`r1` 的常量未读取，仍占源码 local 槽；`r5` 的
`#sizes - 1` 和随后三个数组分别由闭包捕获。完整 statement 帧现保留该未读取槽，
同时按原 capture 源寄存器绑定闭包；不能删掉常量后仅按 SSA 重新排列后续声明。

## 独立开放调用 helper

`statements/open_packs.rs::open_call_statement` 消费已由共享表达式栈建立的
callee／固定参数，从首个开放 CALL 开始，只沿同一 block 的紧邻消费 CALL 链前进。
每一段都验证 open SSA 来源唯一、不是入口 pack、producer 的指令／起始槽／block
完全相同，并由已有 `owns_open_pack` 确认独占消费。

低于 producer 首结果槽的固定参数保持原顺序，未知宽度结果通过
`HirValuePack::expanding` 表达。链必须在 active frame 的基槽完成忽略结果、单值
声明候选、固定多值声明候选或原三值 iterator 协议。固定多值的结果范围必须从外层
callee 同槽开始，原始宽度和最终消费 CALL 的 definition 一并交给父帧绑定；不能
把每个返回值拆成单独调用。非紧邻消费、合流来源、开放 RETURN 和其它中间
普通指令不进入该 helper。失败不提交 pc 或 pending 栈，完整后缀及 maxstack 仍由
父 statement 事务验证。

共享 table 解析仅在该 NEWTABLE 的 SSA 后续确有同 owner SETLIST 时启用开放尾，
避免把真正 `{}` 误识别为开放构造。remap 递归处理真实 call／table tail，不制造定宽
局部或人为 nil padding。完整 source-frame HIR／AST 保护仍覆盖所有后续槽写入。

## 原创验证

`regress_431_open_statement_packs.lua` 及 `tests/support/open_statement_packs.rs`
包含直接返回表、参数展开、固定参数前缀、多层开放调用链、最终单值声明、空表和
开放表作为单个参数，以及旧端午的未读前缀／连续捕获根。返回宽度覆盖 0–12、
45–105，检查空洞、数组长度、子表身份、GC 弱引用和异常顺序。

当前八组集成测试通过：八种正常源码程序各覆盖 stripped／debug 与三种命名模式，
共 48 次运行比较，并对其中 102 个 builder 比较完整 LIR、常量池和 maxstack。
另有六次篡改 producer／consumer 协议的严格拒绝检查。真实四份输入的结果和原始
dump 位于 `/tmp/unluac-open-pack-audit/`；临时目录可能被清理，此文及源码为永久入口。
