# 数值记录键与父键常量池时点

本轮在既有 418 完整构造区间上扩展 `records.rs`，支持字符串或有限数值常量记录键，
并按原始字段写入顺序保留重复键。记录字段仍生成带键的 constructor 项，不改成数组项；
`NEWTABLE` 的 hash 分配提示按全部 `SETTABLE` 写次数证明，包括被后续同键覆盖的字段。

Lua 5.1 的 `recfield` 在编译字段值前调用 `luaK_exp2RK` 处理键。字符串常量只由自身
索引决定 RK／LOADK；数值键在池已满时即使复用低索引常量，也会占用一个 LOADK 槽。
字段值随后从下一个临时槽开始。有限数值的语法不会引入额外运行时指令，inf／NaN
不在这份证明中。数值 RK 键还校验字段开始时的池状态，不能用字段求值结束时的状态
代替，因为嵌套字段可能在中途填满常量池。

嵌套字段还有一个原有 prefix 扫描看不到的时点：父键索引恰为 255 时，源码编译器
已经在子表之前将键加入常量池，而首次携带该键的原始指令是子表结束后的父
`SETTABLE`。回归 419 在 240 次独立字符串前缀后出现以下形状：

```text
NEWTABLE child
LOADK value <- 低索引数字     -- 父键已使池满
SETTABLE child[低索引字符串] <- value
... 子表其余内容 ...
SETTABLE parent[k255] <- child
```

恢复器仅从当前 owner 的首个父字段写取得键的入池事实；写入必须消费当前子表槽，
clone 试解析必须恰好在同一个指令位置闭合，才提交整段表达式。这一证据沿嵌套解析
传递，不查询最终常量池大小。把该父写入键改成已有低索引键的字节码反例，即使
后缀最终包含超过 256 个常量，也必须拒绝。

原创回归为 `tests/regress-case/regress_419_numeric_record_keys.lua`；额外定向验证位于
`tests/support/record_key_regions.rs`。内容包括正负和分数键、数组长度与空洞、nil／false
记录值、数值常量复用、240–270 项前缀跨 RK 边界、重复键对象存活、环境查找触发 GC
和字段调用异常。每种正常样例同时比较 Lua 5.1 运行结果、完整 builder LIR 与常量池
序列，覆盖 stripped／debug 两种输入及三种命名模式。实际游戏输入只允许反编译和
重编译验证，不执行游戏代码。

419 的八组定向测试已通过：23 个正常源码形状、两种 debug 状态、三种命名模式，
共 138 次执行与完整指令／常量序列／frame 比较，另有 20 次严格拒绝检查。
其中原 418 数值记录子表负例已迁为正向语义测试；已有重复键／数值键负断言需要
随能力范围更新，分配错误和观察未完成根的原负例继续保留。

## 固定调用键与闭包字段

头文件的下一类形状为 `[GetText(32, id)] = {...}` 和 `callback = function(...) ... end`。
调用键使用独立 `CallKey` 来源，不把所有计算结果都当作键：只允许全局 callee、连续
字面量参数、单个固定返回槽，调用后必须立即从下一槽分配子表。子表解析还要证明
键槽在整个值构造期间保持，最终由同一个父 `SETTABLE` 消费；键为 nil 时，报错仍
发生在字段值构造完成之后。任意 global／算术键、运行时参数和开放调用仍未开放。

闭包字段只允许 `Fresh` 创建，捕获来源只能是当前构造根以下的既有帧槽或既有
upvalue。原始 closure lowering 必须仅生成一个表达式，不得需要额外绑定声明或
capture barrier；创建后必须立即写入该字段。区间内表或临时槽的捕获不在这份证明中。

原创 `regress_423_record_closures_call_keys.lua` 验证共享可变参数、外层 upvalue、不同
调用间的新闭包身份、对象／数字／重复调用键、弱表存活以及调用异常与 nil 键写入
异常的时点。`tests/support/record_closures_call_keys.rs` 另覆盖直接子数组的 0／1／49／
50／51／100 项边界和篡改 key 槽、返回宽度、子表槽的拒绝。

423 的七组定向测试已通过：15 种正常源码形状各检查两种 debug 状态和三种命名模式，
共 90 次执行／完整 LIR／常量序列／frame 比较，另有 16 次拒绝检查，包括开放调用键及闭包改为捕获
当前构造区间槽的篡改变体。直接子数组的末项调用显式限定为单值，开放尾项未列入
本次完整记录区间能力。

## 非有限记录值的审阅结果

本轮审阅确认官方 Lua 5.1 可将 `1e999` 编译成单条 LOADK，而当前 Generate 将它
拼为 `(1/0)`。在大常量池中后者需要额外 LOADK 和 DIV，可能清除仍存活的临时对象。
合成 GC 反例先计算 `sum = Catalog.left + Catalog.right`，接着写入 `n = 1e999`，再于
读取 `Probe` 全局时强制 GC：原源码输出 `live`，旧生成源码输出 `dead`，两者均正常退出。

`records.rs` 现拒绝非有限的 LOADK 值、RK 字段值及算术常量；固定字段调用的字面量
参数也只接受有限数值。新增负回归覆盖正负 infinity、小池 RK／大池 LOADK，以及带
嵌套 SETLIST 的上述 GC 原型。**纯 record 不含 SETLIST 的版本仍可经过既有 generic
fallback 生成错误源码**；这属于尚未解决的通用非有限常量生成／物理槽问题，不能把本次
事务拒绝说成整个 decompiler 已解决。

只读验证显示仓库官方 Lua 5.1–5.5、LuaJIT、Luau 都接受 `±1e999` 并运行得到 `±inf`；
PUC listing 为独立 LOADK。将临时生成文件的 infinity 拼写改成这类字面量，可恢复该
GC 原型的 `live`，但 generic fallback 的其它指令仍不同，尚不足以证明通用生成改动。
本轮未修改 Generate，也未扩大 infinity／NaN 的构造事务准入。

## 字段调用、低槽运算与原始读时点

三才输入 `f31cefc47987` 的 proto#4 在已有局部帧后，使用 `r13` 构造三项数组。
每个子 record 的 `x/y` 字段先取 `math.cos/sin`，计算低槽参数或通过 MOVE 物化
调用参数，再以单结果 CALL 的同槽结果继续 MUL 和 ADD／SUB；`z` 字段直接由
低槽 `r4` 写入。关键区间为 @102 NEWTABLE 至 @151 SETLIST。

`records.rs` 以连续临时栈恢复这条表达式，例如：

```text
GETGLOBAL r15 math; GETTABLE r15 cos; ADD r16 r5 r11
CALL r15 args(r16) results(r15); MUL r15 r12 r15; ADD r15 r2 r15
SETTABLE r14[x] r15
```

对应 `x = x + scale * math.cos(angle + delta)`。低于根寄存器的操作数只能是
父区间已经证明的 Param／Local／Temp 身份；源码帧的 remap 把它们映射回原槽绑定。
回调可修改同一被捕获变量，因此低槽运算数仍在各自 ADD／MUL／SETTABLE 时读取，
不能提前快照到 callee lookup 前，也不能 inline 其更早的赋值表达式。

新算术分支只接受两个稳定低槽在下一空槽生成结果，或顶部计算值和一个稳定低槽
按原左右顺序写回顶部。已有 RK／大池常量运算仍走原常量来源证明。CALL 必须为
连续固定参数、同 callee 槽单结果，允许全局／链式 lookup callee 及低槽 MOVE callee；
不接受开放结果、额外结果、method 协议或任意嵌套参数调用。小池 LOADK 参数带独立
`Argument` 来源；低槽 MOVE 带 `Copied` 来源。两者不能伪装成 Computed 折入算术，
Copied 也不能折成直接字段值，否则重新编译会删掉物理槽写入。原始直接低槽
SETTABLE 则保留为直接字段值，不添加 MOVE。

原创 `regress_433_record_call_arithmetic.lua` 覆盖六种算术元方法、callee／参数
MOVE、链式 lookup、回调修改被捕获低槽、GC 弱引用及全部 41 个观察点的异常。
`tests/support/record_call_arithmetic.rs` 对 0／248／270 项常量前缀比较运行结果、
完整 builder LIR、常量序列和 maxstack，并提供八次字节码拒绝检查：越界低槽来源、
开放结果、双结果，以及把 `MOVE callee; MOVE arg; CALL` 改成 ADD 后会丢失原始写入
的形状。

当前三种前缀各覆盖两种 debug 状态和三种命名，共 18 次完整运行／LIR／常量／frame
比较，以及八次字节码拒绝检查，全部通过。补充的
`regress_433_upvalue_array_items.lua` 复现套马中的 `GETUPVAL` 固定数组项与嵌套数组，
逐项保留 upvalue 读取；中途调用修改共享 upvalue 时，已物化的前项不随之变化。
该源码另有六种配置的运行／完整 LIR／常量／frame 比较，包含环境 GC、空洞与两次
调用异常，全部通过。初始化在独立返回的 Reset 函数内完成，避免调用者其它临时槽
混入被测构造器的 GC 根。

三才整档在 Simple／DebugLike／Heuristic 三种命名下均 Strict 成功。生成源码按原
GBK 编码重编译后再次 Strict 成功，proto#4 的全部 296 条 LIR、62 个常量字节和
25 个 maxstack 槽逐项一致。真实输入没有执行；临时静态验证入口和日志为
`/tmp/unluac-sancai-static.rs`、`/tmp/unluac-sancai-static.log`，该目录可能被清理。

## 独立的调用者帧 GC 边界

最小合成输入 `/tmp/unluac-caller-gc-minimal.lua` 不含 SETLIST：先执行
`first, second, third = Box(1), Box(2), Box(3)`，再调用闭包清空后两个捕获变量，
强制 GC 并读取弱表。原始源码输出 `1 dead dead`，当前 Strict 生成源码输出
`1 live dead`；生成的第二个 Box 结果仍被额外 caller local 保留。

已从本轮修改前的 HEAD `fe7392ad2359ea1b94f6323b201096807338d459` 导出独立源码树
并离线构建，使用同一官方 Lua 5.1 输入与运行器重新验证：该基线也 Strict 成功并
输出 `1 live dead`。因此这是本轮之前已经存在的 generic 调用者帧 GC 问题，本次
upvalue 数组扩展没有解决它。基线源码树、生成结果、退出码和输出保存在
`/tmp/unluac-before419-gc/` 与 `/tmp/unluac-caller-gc-audit/results.json`；本轮未修改
该通用多赋值路径，也没有执行真实游戏 Lua。
