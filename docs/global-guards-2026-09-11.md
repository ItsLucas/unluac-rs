# 全局安装 guard 的闭合语句

`array_constructor_regions/global_guards.rs::global_guard_region` 处理无参数全局调用
控制单次全局函数安装的五条 LIR：

```text
GETGLOBAL rN <- Predicate
CALL rN args=0 results=1
Branch truthy(rN) -> closure, false -> successor
CLOSURE rN <- fresh child
SETGLOBAL Installed <- rN
successor: ...
```

该模块不重新识别控制结构。已有 HIR 必须恰好包含前两条指令的原始 lowering，以及
一个条件为对应 SSA Temp 的 `HirIf`；If 没有 else，then 语句必须恰好等于原 Closure
和 SETGLOBAL 的 lowering。匹配后才把 callee 合入条件调用，把 closure 合入全局赋值。
callee、调用结果和 closure 定义均要求无 debug local、capture、phi 或区间外使用。
闭包只能 Fresh 创建，并捕获较低帧槽或已有 upvalue。

truthy 路径必须紧随 Branch，false 路径只能到这次 SETGLOBAL 后的第一个指令。
返回的 closed region 到该 successor 为止，不消费或跳过任何后继语句。父 ledger
负责核对起点 active frame，以及后续捕获常量／闭包安装是否一直组成可证明的完整
后缀；某个 raw guard 的外形成立不代表整个后缀安全。

原创回归 `regress_427_global_guard_regions.lua` 包含 nil、false、true、零和对象返回，
环境 lookup／store 触发 GC，以及 predicate 和全局安装抛异常的路径。扩展源码在
guard 后再声明四个捕获常量并安装 closure，验证 false 和 true 两条路径进入同一个
后继源码帧。所有正常 case 使用已有共享入口比较执行、完整 builder LIR、常量池和
maxstack，覆盖两种 debug 状态与三种命名模式。

427 初始阶段的四组测试覆盖八种正常源码形状，共 48 次执行／完整 LIR／常量池／frame
比较；当时的六次拒绝分别是带参数 guard、反向 guard 和后继固定调用。
后续完整 SourceFrame 已独立证明这三种整段形状，旧负例已迁为 18 次运行及完整
指令对照，不能据此放宽局部 `global_guard_region` 的证明边界。测试不执行游戏代码。
