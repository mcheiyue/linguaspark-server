# 数字候选遗漏修复：本地验收（2026-11-07）

部署状态：已于 **2026-11-07 15:48 UTC** 完成 Linux 隔离验收、生产部署与清理；11 月 8 日记录时尚未提交，服务层基线 `4535b2e` 不含本修复。完整生产证据、跨平台边界与回滚见 [Workbase 部署记录（私有仓库）](https://github.com/mcheiyue/workbase/blob/main/docs/deploy.md)。以下保留本次本地验收的原始口径，“本轮未访问 InstaBox”等描述仅指该本地阶段。

2026-11-09 版本归档：本修复与完整 vendor、相对路径依赖、锁文件及本页列出的回归材料一同提交；数字补丁的独立暂存快照通过核心 72 项和服务 17 项测试。后续 Lingua 检测器在单独提交中留档，具体版本以 Git 历史为准。

## 根因与修改

原句见 `numeric_corpus.json` 的 `retry_after_sentence_en_zh`。原版 core `269605c` 的词法候选
集合遗漏目标词片 `0`，在生成 `360` 后只能选择 `00`，造成 `3600` 被翻成 `36000`。
此前全词表对照已证明相同前缀下正确词片分数更高；详细定位保存在
`C:/Users/admin/AppData/Local/Temp/opencode/linguaspark-number-core-20261107/DIAGNOSIS.md`。

本轮在 `vendor/linguaspark` 保留固定上游源码，通过根 Cargo 相对路径使用该核心：

- `src/text.rs`：加载词表时缓存去掉一个前导 SentencePiece 词边界后为纯 ASCII 数字的普通/用户定义词片。
- `src/runtime.rs`：批次任一输入含 ASCII 数字时，取目标词表缓存。
- `src/model/shortlist.rs`：在原有 8 倍数补齐前合并数字词片，保留原有去重和排序。

未改模型、分词、解码或译文后处理；纯无数字批次不增加候选。不同源/目标词表时使用目标 ID。
原 Cargo 缓存没有修改，传递依赖没有升级。后续提交须同时纳入根 Cargo 清单/锁文件、完整
`vendor/linguaspark/` 及本次 QA 文件，不能只提交两个 Cargo 文件。

## 验收结果

- 核心 72 项测试、服务 17 项测试通过；核心与服务的 Clippy `-D warnings` 通过。
- 核心候选覆盖测试先失败再通过；旧版真实 HTTP 也复现 `36000`，修复版返回 `3600`，原句其余译文字节相同。
- 固定 15 项数字样本包含中英双向、小数、负数、金额、日期、时间、批量和 HTML。目标错误修复，其余 14 项译文逐字未变。
- 字面检查为 12/15 通过。其余三项仍按原样记录失败：英译中 `36,000 → 36000`，以及普通/HTML 日期 `2026-11-07 → 2026年11月7日`；人工核对数值未变，未修改检查器去隐藏这些格式差异。
- 12 项真实接口契约检查全部通过，涵盖混合语言、批量顺序、HTML 排除区、空白、数字占位符/表情和输入上限。
- 通用语料及 20 个突发请求的译文与旧版全部一致，HTTP 错误为零。旧通用语料中任意模板变量、行内代码和金额的四项保护失败仍然存在，未作为本轮修复通过的证据。
- 独立源码快照包含 43 个构建/QA 文件，逐文件哈希一致；在解包目录执行 `cargo check --locked --offline` 通过，`cargo tree -i linguaspark` 明确解析到快照中的 `vendor/linguaspark`。构建缓存复用当前 `target`，不声称无缓存全量重编译。
- Oracle 对本地源码及候选交付审查通过。Rust LSP 超时、Python LSP 未安装；以上以实际编译器、Clippy、测试与模型接口验收为证。

## 本机性能

同一台 Windows / Ryzen 5 5600X、相同七份模型文件、两个 worker；编译结束后串行测量。
每个版本每组热态样本 100 次，保留全部响应。下列数据只用于判断本补丁的本机成本，不代表
InstaBox 实测或插件整页体验，也不据此宣称普遍提速。

| 指标 | 修复前 | 修复后 |
| --- | ---: | ---: |
| 通用短文本 P50 | 23.497 ms | 23.733 ms |
| 通用短文本 P95 | 36.764 ms | 29.905 ms |
| 含数字双向短文本 P50 | 18.606 ms | 17.749 ms |
| 含数字双向短文本 P95 | 33.359 ms | 35.965 ms |
| 20 请求、并发 4 总耗时 | 93.530 ms | 87.374 ms |
| Windows 进程峰值工作集 | 176.68 MiB | 176.45 MiB |
| 测试结束时进程 Private Bytes | 147.53 MiB | 147.78 MiB |

通用热态中位延迟变化约 +1.0%，未见明显性能回退。旧进程存活时间更长，当前工作集会受系统
回收影响，因此不拿两个当前工作集直接比较内存，也不拿累计进程 CPU 时间作吞吐结论。

## 产物与复现

证据目录：`C:/Users/admin/AppData/Local/Temp/opencode/linguaspark-numeric-fix-20261107/`。

- `before-numeric.json`、`after-numeric.json`：全部数字原文、译文及原始响应。
- `after-contract.json`：真实协议验收。
- `before-general.json`、`after-general.json`、`before-numeric-perf.json`、`after-numeric-perf.json`：性能原始数据。
- `verification.json`、`verify_results.py`：修复及其余输出未变的可执行断言和汇总。
- `manifest.json`：旧程序及七份模型 SHA；`release-build.json`：工具链、命令、双平台产物 SHA。
- `source-snapshot.tar.gz`、`source-snapshot-manifest.json`：完整构建源码快照；此说明文档是在快照验收后新增的，不在该快照中。
- `process-memory.json`：进程内存计数及比较口径。

Windows 候选 SHA-256：`fc8baa0fe1ae1ae68a8e99c46d8c204c07e392f13538288b67c6466d27829f01`。
Linux musl 候选 SHA-256：`8d6e0ba80ee0909ecac0e2ba60086eb5c8608d2b7fa1658d4c537aa5fe283182`。

```powershell
cargo test --locked --offline -p linguaspark
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
cargo clippy --locked --offline -p linguaspark --all-targets -- -D warnings
python -B qa/numeric_regression.py --self-check
```

证据目录中的 `start-before.cmd` / `start-after.cmd` 固定模型目录、两个 worker 和
`127.0.0.1:19351` / `127.0.0.1:19352`。按工作区 WMI 方式启动候选后，在仓库目录运行：

```powershell
python -B qa/numeric_regression.py --engine lingua --base-url http://127.0.0.1:19352 --output C:/Users/admin/AppData/Local/Temp/opencode/linguaspark-numeric-fix-20261107/recheck-numeric.json
```

完整字面检查会因上述三项既有格式变化返回非零，不应宣称全绿。

验收结束已停止本轮两个测试服务，19351/19352 监听与启动包装进程均释放，生成的 QA 字节码已清理。
本轮未访问 InstaBox。Linux 候选仅完成交叉构建，尚未在 Linux 上运行此补丁；没有重新执行插件整页验收。
本修复只恢复数字词片的候选资格，不能保证任意数字或其他语义永不误译。
