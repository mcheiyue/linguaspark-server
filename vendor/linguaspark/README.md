# LinguaSpark 核心本地副本

来源：[LinguaSpark/linguaspark](https://github.com/LinguaSpark/linguaspark)，固定上游提交
`269605c0a6e78697d94162b71dd97a9d98885276`（简称 `269605c`）。保留上游
`MIT OR Apache-2.0` 许可证元数据，在该版本之上应用以下本地补丁。

该固定上游提交未附许可证正文或 `NOTICE`。本副本按 Apache-2.0 选项分发，
随附的 [LICENSE-APACHE](./LICENSE-APACHE) 为 [Apache 官方全文](https://www.apache.org/licenses/LICENSE-2.0.txt)，
不改动 Cargo 的双许可证声明；修改过的源文件已标记本地改动。

本地补丁只扩展含 ASCII 数字输入的词法候选集合。加载词表时，缓存去掉一个前导
`U+2581` 词边界标记后、非空且全部为 ASCII 数字的 SentencePiece ID。批次中任一输入
含 ASCII 数字时，把目标词表的缓存 ID 合并进原候选集合，再执行原有的 8 倍数补齐。
不同长度和切分方式的普通数字词片及用户定义数字词片均覆盖；控制、废弃、字节及混合
标点的词片不加入。纯无数字批次沿用原候选行为。

根服务通过相对 Cargo 路径依赖本目录，因此补丁可复现，无需修改公共 API 或逐请求
扫描整个词表。此修复解决数字词片被候选筛选排除的问题，不保证模型所有数字译文正确。
