# HIST-005 时间划分合同

更新日期：2026-08-12

范围：只根据 HIST-004 Feature Snapshot 的 `series_id` 与 `scheduled_start_utc` 生成 train、validation、calibration 和 sealed final test manifest；不读取 feature value、比分、winner 或市场字段。

## 1. 时间窗口

Rust 合同位于 [`src/temporal_split.rs`](../src/temporal_split.rs)，真实构建入口位于 [`research/build_temporal_split_dataset.ps1`](../research/build_temporal_split_dataset.ps1)。

- 所有窗口使用半开区间 `[start_utc, end_utc)`。
- 顺序固定为 train → validation → calibration → final test。
- 相邻窗口必须首尾相接；间隙、重叠、倒置或空窗口全部 fail closed。
- 每个 `series_id` 必须唯一，且 Scheduled Start 必须恰好命中一个窗口；不允许随机打散或按小局拆分。
- 默认边界只选 UTC 自然日，至少要求 4 个不同日期；不会为了凑比例把同一天的 series 分到两个集合。

当前 23 场样本的窗口为：

| Split | 半开 UTC 区间 | Series |
|---|---|---:|
| train | `[2026-08-08, 2026-08-09)` | 3 |
| validation | `[2026-08-09, 2026-08-10)` | 7 |
| calibration | `[2026-08-10, 2026-08-11)` | 6 |
| final test | `[2026-08-11, 2026-08-12)` | 7 |

这只是对当前小样本验证时间划分合同，不代表 3 场 train 足以训练模型。

## 2. Final test seal

调参阶段的 `TemporalSplitManifest` 明确列出 train、validation 和 calibration IDs，但 `final_test` 类型中不存在 `series_ids` 字段，只保存：

- 半开时间窗；
- `series_count`；
- 对按 `(scheduled_start_utc, series_id)` 排序后成员的 SHA-256 commitment；
- `access_policy = sealed_until_model_freeze`。

commitment 使用 RustCrypto [`sha2`](https://docs.rs/sha2/latest/sha2/) 的标准 SHA-256 `Digest` API。`release_final_test()` 只有在调用方提供冻结的 model artifact、model config 和 evaluation code SHA-256 后，才从相同 source dataset 重新计算成员、核对 count/commitment 并返回显式 IDs；源数据漂移时拒绝 release。

seal 是防止标准开发流程意外提前消费 final test 的工作流门禁，不是针对有权直接读取 HIST-004 源数据人员的加密保密机制。

## 3. Lineage 与构建

```powershell
./research/build_temporal_split_dataset.ps1 `
  -Version "2026-08-12.e678afb.hist005"
```

HIST-005 没有直接 raw input，其 Dataset Manifest v1 通过 `upstream_datasets` 固定 HIST-004 manifest/output 路径和 hash；Manifest 合同允许“至少一个 raw input 或 upstream dataset”，但不能两者同时为空。

真实产物：

- dataset：`data/processed/lol-temporal-splits/2026-08-12.e678afb.hist005/temporal-split-manifest.json`
- dataset SHA-256：`fefdb5ec783d12d73721f0fe05f71cc6ccfd6aefa56c588d372bc24c84f8cb1d`
- manifest SHA-256：`7bee9401986004e0fbcc9ec65c9838c276568104c3ce176710fbd13654b2a4a1`
- final test membership SHA-256：`c1965fff7cbeb0cece75b7a1e4429c1d9e7b65699b1aa8cf6cff44811137583e`

## 4. 验证与边界

- 6 个定向测试覆盖确定性排序、final IDs 不序列化、间隙/重叠、重复/边界外 series、空集合、冻结授权以及 source membership 漂移。
- 真实输出覆盖 23/23 series，development 公开 16 个 ID，final test 仅有 7 行 commitment，输出 JSON 不含 final test IDs。
- 同输入缓存重放必须产生相同 dataset SHA-256。
- 当前数据只覆盖 4 个 UTC 日期；HIST-006 必须把样本规模与时间覆盖不足列为 Gate 阻塞证据。
