# Lua 5.1 记录表算术字段恢复

2026-09-09 已与后续捕获数组、RK 键恢复整理为本地基线提交。下文保留本轮当时的
验证结果；最新基线为 [RK 记录键恢复](rk-record-keys-2026-09-09.md) 的 45 成功 / 66 失败。

以本轮修改前的 release CLI 为基线，在同一份
`../jx3_exp_src/decompile-errors` 归档上按 Lua 5.1 / GBK / Strict 复测：

| 指标 | 修改前 | 修改后 |
| --- | ---: | ---: |
| 原始归档 | 112 | 112 |
| SHA-256 唯一输入 | 111 | 111 |
| 严格生成源码且 Lua 5.1 重编译通过 | 30 | 39 |
| 严格失败 | 81 | 72 |

原有 30 份成功逐 SHA-256 保留。新增 9 份：

| 脚本 | SHA-256 |
| --- | --- |
| 少林·韦陀献杵增加伤害 | `c0da1b1bb44caf079b6ed6ca230780aeb7c9cca4b71f9c0a4527debba32f66f9` |
| 少林·韦陀献杵增加伤害 | `717e4335b648d4b21256c5b3d630f45a3438cb1a2cac73e84140718e25d1722e` |
| 少林·韦陀献杵增加伤害 | `c0b17bbb0b3e2ae2a6eb83a6b094327e66f72bc411917ef19577eb70e69e9827` |
| 少林·韦陀献杵增加伤害 | `d48cd67b46810cf464e8a642b2917ccbf98f3c329d0e4d38d7c449d01904ee7a` |
| 少林·韦陀献杵增加伤害 | `f4c4b92fd4aa075f215e6b47e005dc7361dd630b19b8d32f1fcbf2eebb3abd93` |
| 少林·韦陀献杵增加伤害 | `3690ae06b6defa2207196e535e932dec3b733d5e9c2c51f650c2670afa30f266` |
| 少林·韦陀献杵增加伤害 | `94321ab08e6b3490980aa3ee8730b2586452a38d8ef48613177b8d11a960daee` |
| 天策·沧月 | `875c88811aed20b30ac588509ddabab77653744f7404ccf0294c1eecae40b06e` |
| 天策·沧月 | `ad44b847fae2c48730ff0e2847b46443baa8a7d9f2081061c691815c427cbe44` |

## 恢复边界

扩展 `array_constructor_regions/records.rs`：已有字段读取表达式可以在同一个
scratch 寄存器中，依原顺序与数字 RK 常量做算术。常量可以位于左侧或右侧，
表达式保留原结合结构，不交换操作数、不重排运算、不复制字段读取。

仍要求单块完整区域、立即全局安装、字符串唯一键、原 NEWTABLE 分配参数和
SETLIST 批次；中间定义不能被捕获或逃逸。两个寄存器操作数、重复 scratch
操作数、非数字常量和写入其他 scratch 位置均不接受。局部捕获表、分支中
构造、混合嵌套和开放返回值没有新增支持。

## 验证

- 库单测：56/56。
- 记录表集成测试：6/6。新增 30 组执行及指令/常量差分，覆盖三种命名、
  stripped/debug、六种算术元方法、12 个逐点异常、49/50/51 项批次及 GC 观察；
  并保留既有 42 组差分及拒绝测试。
- 原创 `regress_412_record_arithmetic.lua` 已接入常规矩阵：2/2。
- 9 份新增真实归档的前缀和表构造区间回编译后共 500 条 LIR 指令逐条相同，
  根 proto 常量也逐项相同（字符串按原 GBK / 生成 UTF-8 解码比较）。
- 未执行游戏字节码或生成的游戏源码；真实归档的运行时行为没有实机确认。
- 修改文件通过 rustfmt 检查；全仓 `cargo fmt --all -- --check` 仍有既存格式差异，
  涉及本轮未修改的 analyze/mod.rs、decision/mod.rs、table_constructors 等文件。

复现归档统计（从工作区根目录执行）：

```sh
cargo build --release --manifest-path unluac-rs/Cargo.toml -p unluac-cli
python3 jx3unpacker/tools/lua-compat/check.py \
  --corpus jx3_exp_src/decompile-errors --no-fixtures --modes local \
  --unluac-cli unluac-rs/target/release/unluac-cli \
  --report /tmp/jx3-arithmetic-after
```

本次比较的原始结果分别在 `/tmp/jx3-arithmetic-before/results.jsonl` 和
`/tmp/jx3-arithmetic-after/results.jsonl`，临时目录可能被系统清理。
以上是本轮当时的候选结果；现已整理提交，未推送或更新解包器固定依赖。
