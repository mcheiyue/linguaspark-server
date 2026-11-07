# Fork 质量与性能验收（2026-11-07）

结论：服务层切句、批量重组与结构保护修复了本次复现的长文重复、漏段和换行丢失；短句与原生批量性能接近上游，低内存优势保留。允许进入 Workbase 接入验证。尚不能据此宣称通用语义质量超过 MTran，或真实网页的 429 已解决。

## 固定条件

- 服务基线：`LinguaSpark/server` 提交 `edd16a674b1484490a4a29d588dab9c1a191e6c3`，Fork 为其上的本地补丁；未创建提交或远端 Fork。
- 推理核心：`LinguaSpark/linguaspark` 固定 `269605c`，未修改核心。
- MTran：`xxnuo/mtranserver@sha256:1c1d57cfa66f6d3127ac7e224d04a7b1beeb1fa6a771d6ae275baee3aca86afe`，v5.1.0；隔离实例设置 `MT_CACHE_SIZE=0`、`MT_WORKER_IDLE_TIMEOUT=1800`，生产实例未重启。
- 三者使用同一组 Mozilla en→zh 原始权重和词表；MTran 使用已有的 ONNX 转换产物，推理与数值实现并不相同。
- InstaBox 4 vCPU / 8 GiB，三者依次运行、仅回环访问，CPU 上限 4 核、内存上限 2 GiB、禁用 swap；Lingua 原版与 Fork 同为本机交叉编译的 MUSL Release，4 个执行器。
- 5 条热短文本、两篇 340/356 词长文、3 项原生批量，以及 20 条不同短文本的并发 2/4 各跑 3 轮。文件缓存未清空，启动不等于完全冷启动。
- 语料 SHA-256：`af46291d3cfde673b3e1a658117bc272578932bd466842dba2f24c000c0d195a`。这些是开发中反复使用的回归样本，不是独立盲测或通用质量榜。

原始文件 SHA-256：

| 文件 | SHA-256 |
|---|---|
| model.enzh.intgemm.alphas.bin | `4e5accc141373565ddc8fa1565bceaa8d0c3482a82cab8131c719ebcc6c2157c` |
| lex.50.50.enzh.s2t.bin | `4a5e5827788060f1d718a8132b69440929387514a045796e9b77f935db68c055` |
| srcvocab.enzh.spm | `bd9b65504acc6d9726dd281f7defc2adb7c2c22d0688fe2f84697de25197c8c5` |
| trgvocab.enzh.spm | `aded6993c36e440284d11cec3f6b8aef9c0e43188a772d80be342a713adf223d` |

## 同机回环结果

| 指标 | 原版 LinguaSpark | Fork | MTran |
|---|---:|---:|---:|
| 热短句中位耗时 | 16.84 ms | 17.21 ms | 15.28 ms |
| 长文 1 中位耗时 | 491.44 ms，严重重复漏译 | 180.04 ms | 155.99 ms |
| 长文 2 中位耗时 | 220.93 ms，严重漏译 | 194.07 ms | 169.60 ms |
| 3 项原生批量中位耗时 | 9.93 ms | 10.37 ms | 13.43 ms |
| 20 请求、并发 2，中位总耗时 | 85.39 ms | 91.78 ms | 105.57 ms |
| 20 请求、并发 4，中位总耗时 | 72.71 ms | 73.89 ms | 106.67 ms |
| 进程 RSS / PSS 采样峰值 | 83.73 / 83.72 MiB | 82.39 / 82.38 MiB | 365.66 / 359.36 MiB |
| cgroup 生命周期峰值 | 94.56 MiB | 95.32 MiB | 348.86 MiB |
| 启动到健康 / 第一笔请求 | 2.11 s / 28 ms | 1.05 s / 19 ms | 2.78 s / 4.78 s |
| HTTP/采样错误、OOM、swap | 0 | 0 | 0 |

Fork 相对原版：热短句约 +2.2%，3 项批量约 +4.4%，并发 2 的 20 请求约 +7.5%，并发 4 约 +1.6%。这些小样本差异不构成显著性结论。MTran 在本轮串行短句/长文更快；Fork 的收益是修复原版长文错误，并保留批量、并发与低内存表现。

RSS/PSS 每 50 ms 采样，短请求可能落在采样点之间；cgroup 包含页缓存且共享页归属不同，不能把两种峰值相减当独占内存。CPU 分阶段时间和平均核数保存在原始 `phases` 中。此次没有测公网、浏览器插件或与 ASR 同时计算。

## 质量与边界

- 两篇长文逐句核对后恢复三段原文覆盖，原版的长串重复和大段遗漏消失；换行、首尾空白和空白输入得到保留。
- HTML 保留标签/属性语义、注释及 script/style/pre/code/translate=no/notranslate 区域；HTML5 解析会规范化引号和不完整标签，不承诺逐字节原样 HTML。
- 数字占位符、表情与围栏代码在确定性边界外保护。任意 `{{name}}`、`${region}`、URL、金额和 Markdown 行内命令不承诺逐字保真；原始失败样本未删除。
- 技术短句的条件与否定正常。`Deep thought`、`being available/present`、`those who know the most` 和指代仍有双方共同误译。超长重复单句虽然保留结尾否定，仍出现“重启前/后”错位；不能把结构检测通过当语义合格。
- 本次只有 en→zh 模型的质量/性能对照；自动识别的混合输入按项处理，源/目标相同直接透传。后续 Workbase 接入已补中文→英文短句隔离和生产验收，不构成中文长文质量评测。
- 16 项 Rust 行为测试、Clippy、Windows/Linux Release 已通过；12 项真实 HTTP 契约检查在 Windows 和 Linux 均通过，涵盖混合自动识别、语言拒绝、批量顺序、保护内容以及 128 项/1 MiB 上限。

## 证据与复现

- 本机完整报告：`C:/Users/admin/AppData/Local/Temp/opencode/linguaspark-fork-batch-20261107/results/{upstream,fork,mtran}.json`、对应 `*-qa.json`。
- 人工逐项对照：同目录上级的 `translations.md`；原始结果保留全部失败项。
- 脚本、语料、二进制哈希：同目录 `manifest.json`；Fork Linux SHA-256 为 `af6e0d8dc03c16893e9d1be05ac8ea65316efc30e0ae16ddd1bed06a7a4af02c`。
- 沙箱：`/data/linguaspark-fork-20261107/results/`。测试单元/容器已停止，模型与配置副本已清理；生产 MTran 启动时间和模型哈希不变，Workbase health=200。二进制和测试包已在 00:17 UTC 接入阶段收尾时清理，保留约 1.5 MiB 结果与复现材料。
- `qa/compare.py` 对 Lingua 使用 `/translate` 的 `text` 数组；MTran 批量必须用 `/translate/batch` 的 `texts`，返回 `results`。早期脚本用错接口导致的 500 已排除，未计入能力差异。
- `fork-contract.json` 含 1 MiB 超限输入，文件接口默认取回被截断为 262144 字节；本机此份不是完整 JSON。Linux 实际执行退出 0 且打印 `passed=true`，后续取证应导出不含大输入的摘要。

本报告主体是隔离对照。后续 Workbase 接入已于 2026-11-07 00:13 UTC 切换生产至同一 Fork 二进制，双向/别名/权限/SSE/突发请求验收通过，MTran 保留作回滚；详见 `../../../workbase/docs/deploy.md` 的当日生产记录。DeepLX 与 `/imme` 的 Workbase 入口仍是后续扩展。
