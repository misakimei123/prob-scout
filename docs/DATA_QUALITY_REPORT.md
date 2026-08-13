# HIST-006 数据质量报告合同

更新日期：2026-08-12

## 1. 结论

`HIST-006` 初版已完成，当时 M2 Gate 结论是 `NotReadyForM3`。该历史结论已由 HIST-010 的 1,778-row 重建 supersede 为 `ReadyForM3`；本文件以下内容保留初版证据快照，当前状态见 `HISTORICAL_IDENTITY_EVIDENCE.md`。

- Eligible series：`23/500`（`4.60%`）。
- Event 时间：`2026-08-08T15:00:00Z` 至 `2026-08-11T20:00:00Z`，仅 `4` 个 UTC 日期、`1` 个年份。
- Patch：仅 `26.15`，`23/23`（`100%`）。
- 赛区：6 个；EMEA 9、Brazil 6、Korea 4、North America 2、Asia Pacific 1、China 1。
- 必填 Series/Feature 字段缺失：`0/23`、`0/46`；晚于 `T-15m` cutoff 的 feature source time：`0/46`。
- Same-Patch history unavailable：`3/46` team sides（`6.52%`）；保留 series 并降级该特征，不解释为 `0%` win rate。
- DATA-009 execution-grade market snapshot 缺失：`50/50`（`100%`）；全部 Grade C，只允许 signal research。
- IQR review：`same_patch_series_count` 4 个、`rest_minutes` 2 个；异常标记不自动等于错误或排除理由。

因此当前数据只证明历史 pipeline、时间防泄漏和 split seal 的机械正确性，不能支持模型有效性、跨 Patch 稳健性或历史可执行 PnL 结论。`MODEL-001` 继续被 M2 Gate 阻塞；下一步应先扩展不可变历史语料并重建 `HIST-002`–`HIST-006`。

## 2. 构建入口与产物

构建命令：

```powershell
./research/build_data_quality_report.ps1 -Version <new-version> -MinimumEligibleSeries 500
```

脚本执行以下 fail-closed 流程：

1. 验证 HIST-003、HIST-004、HIST-005 的 Dataset Manifest v1、output 路径与 SHA-256。
2. 将 DATA-009 固定为 content-addressed `data/raw/data_quality/review/` snapshot。
3. 由 Rust 同时校验 series 完整性、跨数据集成员全集、Feature Snapshot `T-15m` 合同、source time、ratio/count 关系和 temporal split commitment。
4. 对同一规范化输入连续构建两次 Markdown；字节 hash 不一致则不发布 processed artifact。
5. 报告通过必需章节断言后写入 `data/processed/lol-data-quality-reports/<version>/`，再生成并验证 manifest。

本次有效构建版本为 `2026-08-12.e678afb.hist006-v4`：

- 报告 SHA-256：`eddd8534144ffdcd9a1ec0a15052395922a7c3675ede12dc768af1982f8a86a2`
- Manifest SHA-256：`13270e42ee64e403454c6685b7e26acfddad4b4b370cdd4c7b84c093340b9170`
- 相同输入双构建：`DeterministicReplay=True`

`data/` 属于本地可重建数据，不提交仓库；本文件只保存合同、结论和可复核 hash。

## 3. 缺失与降级合同

| 字段或状态 | 判定 | 处理 |
|---|---|---|
| Series identity、Scheduled Start、BO、Patch、双方、比分、winner | 为空、矛盾或跨表不一致 | 排除 series；报告 fail closed |
| Feature source timestamp | 晚于 snapshot cutoff | 排除 series；判定 leakage |
| Prior form | `prior_series_count=0`，source time 与 rest 为空 | 保留 series；显式标记 unavailable |
| Same-Patch form | `same_patch_series_count=0` 且 source time 为空 | 保留 series；只降级 Same-Patch feature，不填充 `0%` |
| Split membership | 重复、遗漏、重叠、窗口外或 final commitment 不一致 | 停止所有下游消费 |
| Historical market Grade C | 缺少决策时点 bid/ask、depth 或 fee | 仅允许 signal research；禁止 execution/PnL 证明 |

## 4. 反方解释与边界

- 23 行内部一致并不代表样本可用于模型比较；低缺失率无法抵消极小样本、四日窗口和单 Patch 偏差。
- IQR outlier 可能来自国际赛程或长休赛期，不应仅凭统计阈值删除；进入模型前需逐行核对来源时间并预注册 robust scaling/cap。
- Exact source-key 历史降低错误身份合并，但可能低估改名队伍覆盖；只有补齐时间化 identity evidence 后才能合并。
- Manifest 证明使用了哪些输入字节和代码，不证明上游页面在比赛时不可变。
