# 非入口完整源码帧事务（422 至 441 阶段）

本轮使收割机归档
`08826fa8002112ffde485c069e730e266b516716cb47092c8ac6ac9544efc80d`
完整 Strict 生成并通过 Lua 5.1 重编译。它的 proto#2 原始与重编译均为 **140 条 LIR、
10 个参数、28 个栈槽**，包括前序所有分支、调用、数组分配和覆盖写，逐条 LIR 相同。
没有执行游戏 Lua。静态比较前将生成文本编码回 GBK，保持归档字符串字节编码。

## 必须一起恢复的内容

收割机 R10=GetPlayer、R11=GetScene、R12=GetQuestIndex、R13=GetQuestPhase 是连续
源码 locals。R11 和 R13 只在条件中读取一次，仍须占据原槽，后续完整调用和数组从 R14
开始。GetDynamicSkillGroup 与 IsCorrectScene 的结果则是条件临时值；把它们错误地
声明成 local 会抬高后续临时栈，完整后缀的寄存器匹配会拒绝该解释。

前序 RemoteCallToClient 的全部 callee、参数和嵌套表必须恢复为一条表达式。其
GetEditorString 结果暂存在 R25；随后第九条子数组的布尔写清除 R25。如果只折叠数组，
前序人工 local 会继续持有旧对象，同时把数组编译到更高的寄存器。

`statements.rs` 因此只在整个 proto 成功解析后原子提交。固定调用结果先尝试源码声明，
再用完整后缀证明该声明占槽；失败才尝试唯一消费的条件临时表达式。分支沿原始边和
词法作用域递归，当前只支持可完全解释的前向结构。表达式代码、帧引用映射分别在
`statements/expressions.rs`、`statements/frames.rs`。

声明直接生成 `HirLocalDecl`，不再依赖后续 Temp 提升。标量计算也先尝试单槽初始化，
只有完整后缀仍匹配才保留；未使用但实际占槽的常量声明同样受保护。`HirSourceFrame` 保留 local 到
物理槽的映射和 maxstack；已证明 proto 跳过 HIR 化简。AST 显式传递 source_frame，
readability 的普通和 scoped walker 均不改写其函数体节点。它们只递归处理其中
尚未认证的子函数体，避免跳过子函数所需的 AST 清理。`FramePinned` 也明确禁止
local 内联，不沿用 `PhysicalRoot` 的相邻 callee 例外。Naming／Generate 照常运行。

## 证明边界

- 必须消费全部原指令，并匹配声明槽、调用参数／结果宽度、表达式寄存器栈和 SETLIST。
- 原 maxstack 必须等于已解释参数／写槽所需的最大值；额外未知物理槽不能冒充已证明帧。
- 422／424 初始阶段只接受固定单结果调用和完整表构造；现在支持同序标量读取、
  计算、常量和原始 LOADNIL 局部组，但仍不生成 dummy nil 补槽。
- MOVE 得到的裸绑定和 LOADK 得到的字面量不能被当成计算结果折进后续算术／查表；
  那样可能让 Lua 编译器改用原 local 或 RK，消掉原始覆盖写。
- 一个计算临时值在算术或条件中必须只消费一次，不能复制完整 RHS 来伪造两个寄存器读取。
- 临时 `not value` 不能直接变成条件真值测试：Lua 编译器可能省掉原 NOT，消除一次
  物理覆盖。条件入口明确拒绝这种解释；退出词法作用域后复用 scratch 的反例仍拒绝。
- 428／431／432 已补齐下述循环、写回、开放包和捕获安装协议；没有覆盖的形状及
  不明帧仍返回原有严格路径，没有 VM、IIFE 或逐项 SETLIST fallback。
- 常量池达到 256 项的整个 proto 暂不取得此类证书；数字／nil／bool 的 RK 选择需要
  独立的逐求值点池状态。所有非有限 Number（包括只出现在比较中的 inf／NaN）也拒绝。

## 422／424 阶段验证记录

- 422：6 项测试，5 份原创源码 × stripped/debug × 3 命名，共 **30 组**运行、完整
  builder LIR、常量顺序和 maxstack 对照通过；10 组边界样例继续拒绝。
- 424：4 项测试，3 份原创源码 × 6 配置，共 **18 组**同类对照通过。覆盖六条提前退出
  路径、两处调用异常、逐 global lookup 的环境元方法 GC，以及单次读取 scene 保活／
  R25 scratch 及时释放。重复条件仍保留原始 LIR，验证后层没有折叠已认证控制流。
- 424 的八组字节码变体拒绝错误 R25 写回、重复临时表达式消费及额外 maxstack 槽。
- 同轮库单测 56/56、记录集成 67/67（另 1 项真实归档测试 ignored）、完整 workspace
  Clippy `-D warnings` 通过。完整矩阵和全归档统计由总任务统一复测，不据此声称全部成功。

```sh
cargo test --locked --offline --test regressions_422_statement_regions
cargo test --locked --offline --test regressions_424_source_frame
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
```

## 后续已验证扩展

428 消费 StructurePlan 已冻结的 generic-for 协议，保留三项 iterator/state/control
隐藏槽及原始 TFORLOOP 写入。隐藏槽在源码帧中不可读；可见迭代变量使用新的 pinned
LocalId。LEN、同序 CONCAT、既有 local 的算术／调用结果写回及动态 indexed assignment
均作为完整语句恢复，数组 RHS 的键求值与构造顺序没有拆开。

代表 skill 头完整 Strict 生成并重编译成功；proto#38 原始和生成均为 **59 条 LIR、
15 个栈槽**，逐条相同。428 的 3 项测试覆盖原形及 1／3 个可见迭代变量，合计 **18 组**
运行、完整 LIR／常量／maxstack 对照；6 组协议／隐藏槽读取／错误写回变体拒绝。

431 的开放结果只在唯一、紧邻的 SSA pack 消费链或完整建表尾项中通过；不展开为
静态 locals。捕获安装逐个沿原 capture.source 映射到当时的 pinned LocalId，子函数
UpvalueId 顺序保留。未读前置常量、LEN/SUB 局部和连续开放数组现在能在同一整段内证明。
四份对应游戏归档均完成 Strict 和 GBK 重编译：两个新版端午的 proto#11 各 **16 条**、
无名剑 proto#9 **41 条**、旧端午 root **86 条**，原始和重编译 LIR 相同。旧端午曾因
普通子函数整理改变首次 upvalue 使用顺序；对无 child、最多 128 条的捕获读取函数也
执行完整帧证明后，其 child#2 **21 条**及父闭包 capture 序列均恢复一致。该额外入口
只接受至少两个 upvalue 的 reader；单 upvalue 不存在首次捕获顺序重排问题，仍使用
原有子函数整理路径。

432 的 numeric-for 同样匹配冻结协议、三项隐藏控制槽和可见 binding。新增同序双
computed 比较与 OR 共享后继条件恢复；每段条件的临时值只读一次，共享 then／return
正文不会复制。OR 支持保持原顺序的 else 分支：424 新增两种 RHS 返回的专门样例，
共 **12 组**完整 LIR／常量／maxstack 和运行结果对照通过，覆盖首项短路、次项成立及
两项均失败。条件专用 inf／NaN、退出作用域后的 NOT scratch 反例也均保持 Strict 拒绝。
431／432 的详细矩阵由各自 support 文件及总任务日志记录。

434 将固定多返回值 CALL 恢复为一组源码局部声明，保留未读结果的真实槽和结果宽度。
固定三结果先在独立候选中验证 generic-for 协议；失败才尝试普通三变量声明，失败
试探不会遗留 binding。根调用后的同槽查表与 LEN 继续作为一个表达式，例如
`local scene_id = npc.GetScene().dwID` 与 `for i = 1, #GlobalRows do ... end`。
434 的游戏及矩阵结果由该项测试与总任务日志记录，此处只记录当前实现边界。

后续标量赋值也沿原槽执行：LOADBOOL 源码 local 保留身份，已存在 local 可直接接受
常量／布尔／低槽 MOVE／查表，以及固定调用后一次算术计算的结果。两个 computed
算术操作数只允许按原始相邻左／右槽顺序合并到左槽；反向排序和重复消费仍拒绝。
若另一种声明解释可以逐条保留两个读取及反向算术操作，仍可取得证书；424 的交换
算术操作数字节码变体要求这种恢复保持完整 LIR，不能用重新排序的单表达式冒充。
全局／索引写回区分直接低槽值与待消费的完整计算结果，不凭赋值语法删除中间 MOVE。

完整赋值先于 local 声明尝试，并验证全部后缀。全局名称必须按原常量池在 RHS 的
新常量之前出现：`Global = {{311, true}}` 与 `local rows = {{311, true}}; Global = rows`
虽然可以使用相同物理写槽，常量插入顺序不同，不能任选一种。424 新增两种源码各六
配置的全指令对照，还加入查表元方法中的 GC、算术各项观察与第二项异常时点对照。

三才化生归档现已完成三个命名模式的 Strict／GBK 重编译；其 proto#4 的 **296 条
LIR、62 项原始常量、25 个栈槽**逐项一致。建表字段调用和后续计算分别由 records
栈及完整语句栈证明，末尾对已存在 local 的调用结果写回也保持原槽。没有执行游戏 Lua。

435／436／437／438 分别补齐共享 false 后继的 AND、冻结循环退出边对应的 break、
比较结果的标准 LOADBOOL diamond、以及当前 then 臂已证正常出口的复用。AND/OR
仍按原顺序构造 then/else；break 必须属于当前循环的已冻结 EdgeTransfer，不能把
任意前向跳转当退出；跨出当前臂的条件目标只有与其既有 continuation 完全相同才
映射为臂结束。进入嵌套循环或分支时保存并恢复各自证书，不能借用相邻作用域事实。

布尔物化保留 false 与 skip 共用原始 LOADBOOL origin、true 写同槽及唯一入口；只把
条件内联成布尔表达式但省去实际写槽仍不允许。调用结果可继续成为同序比较或 CONCAT
的一部分；另保留“固定调用后立即结束 initializer”的完整后缀候选，避免误将真正
源码 local 吞入下一表达式。424 新增调用开头、MOVE／字面量／upvalue 混合拼接的
两种源码，十二配置的全 LIR／常量／maxstack 与运行结果对照通过。

上述扩展完成后，424 独立套件实测 **12 通过、1 个真实归档静态测试 ignored**。
其中八项原创正向测试含十三种源码、合计 **78 组** stripped/debug × 三命名运行和
完整 builder 指令／常量／maxstack 对照。另有交换算术操作数的两组静态对照，以及
错误 scratch 写回、重复消费、额外 maxstack、inf／NaN 和退出作用域的 NOT 边界。
ignored 游戏样例已在本轮通过显式路径单独执行静态对照，未执行游戏 Lua。

439 修正最后输出层的比较打印偏好。原 complexity 启发式会将字段／调用的求值
顺序反转，即使 AST 已取得源码帧证书也会改变临时写槽。共享 pretty helper 现只
允许左侧有限标量字面量、右侧既有 local／param 的比较换成 `>`／`>=`，此形状
只使用同一个常量操作数；没有前层方向证据的 global、upvalue、查表和调用保持 AST 原顺序。
专项样例同时核对原始运行顺序、完整 LIR 和帧宽度，不能仅以打印结果可编译作为验收。

440 在规范化 LT／LE 之外显式传递 `RelationalOperandOrder`，由原表达式区间、
相邻计算槽与常量首次出现顺序决定。两个 computed 的 `>`／`>=` 可保持源码先后，
`0 < FreshCall()` 和 `FreshCall() > 0` 也分别维持常量插入时点；EQ 不用交换两端
修补顺序。该组十一种源码、六配置共 **66 组**运行、常量、全指令与帧对照通过。

441 的声明判断进一步重放初始化式实际的 Lua 5.1 常量插入阶段，不能只检查常量
集合或编号没有缺口。`if 31415 < Fresh(31415, Key) then return rows end` 的调用
区间虽包含全部常量，把它独立声明仍会让函数名先于 31415 插入。父 global／索引
赋值的 key 也须在 RHS 前保留；布尔 diamond 紧接 store 时优先恢复整个 Assigned，
失败再尝试没有父 reservation 的局部值。这样显式 `local result = comparison;
Sink = result` 仍能保持自己的原始顺序。

声明闭合检查覆盖数字延后物化、字符串即时插入、record 键先值后及比较方向，并应用
于固定多结果调用组。固定五份反例 × 六配置复核为 **30 正确、0 拒绝、0 错误**，
全部保留 SourceFrame，运行输出、全部 LIR、常量字节与帧宽度一致；详见本轮统一日志。

原有 422 开放实参、反向 diamond、固定调用写回负例在取得完整证明后迁为正向全指令
对照；while 与不明写回、错误 scratch／pack／loop 协议仍拒绝。上节 30／10 为当时
422 阶段快照，不是这些后续扩展后的总数。所有游戏验证仍只做静态生成和重编译。
