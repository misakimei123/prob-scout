# M3R-004 Recovery Temporal Split

> 状态：Completed
>
> 固定日期：2026-08-14
>
> 范围：只建立新 Development / sealed Final Test 与描述性覆盖；未读取 label，未训练或选择模型。

## 1. 结论

M3R-004 已为 M3R-003 的 3,155 条 eligible series 建立全新的连续时间划分。旧 MODEL-007 Final Test 的 356 个成员不仅未进入新 Final，也未进入新 corpus 的任何 split。Rust 恢复合同重新从旧特征集计算旧 Final commitment，并同时验证成员重叠与时间重叠；两个计数均为 0。

新 Final Test 在开发 manifest 中只公开窗口、701 条计数和 commitment，不公开成员 ID。它仍是工作流 seal，而非对有本地原始数据权限人员的加密隔离；M3R-005 的标准入口不得读取或推导 Final 成员。

## 2. 固定输入与产物

- 新 Feature Snapshot：`data/processed/lol-prematch-features/2026-08-13.f42324d.m3r003-features-v1/prematch-feature-snapshots.json`
- 新 Feature SHA-256：`8433cc10ee73cab042049d0afe0f81cfc0d96504348346178fb6c4baaa3c7f2b`
- Recovery Split：`data/processed/lol-temporal-splits/2026-08-14.3d155d3.m3r004-split-v1/temporal-split-manifest.json`
- Split SHA-256：`ed7564bf68a4e16400c1d712242861a03a32893ecad5a91d814c86c1dcba64b1`
- Split Dataset Manifest SHA-256：`1b50afbd50e1f3fc0013d889946788d33745facd42e38f87faa9a9e91561dbf8`
- 新 Final commitment：`d8b3f5e2cca5eb707173a1ea4a8881c0b9e764173e6d24a373899639fab3a130`
- 旧 Final commitment：`c5b7295b8363bc62c4cbf8d1c0edc798179fa09ad6634060f5207b1397a39f1d`

Dataset Manifest 显式引用新 Feature、旧 Feature 和旧 Split 三条 processed lineage。恢复上下文固定旧 split manifest hash、旧 source dataset hash、旧 Final 时间窗/count/commitment，不保存旧 Final IDs。

## 3. 划分合同

边界在读取 feature value 或 label 前按完整 UTC 月固定，使用连续半开区间：

| Split | UTC window | Series | BO3 | BO5 | BO5 share |
|---|---|---:|---:|---:|---:|
| Train | `[2025-07-01, 2026-01-01)` | 1,281 | 857 | 424 | 33.10% |
| Validation | `[2026-01-01, 2026-03-01)` | 430 | 317 | 113 | 26.28% |
| Calibration | `[2026-03-01, 2026-05-01)` | 743 | 660 | 83 | 11.17% |
| Final Test | `[2026-05-01, 2026-07-01)` | 701 | 501 | 200 | 28.53% |

旧 Final 窗口为 `[2025-05-19, 2025-07-01)`；新 corpus 从 `2025-07-01T00:00:00Z` 开始。恢复合同的 `member_overlap_count=0`、`temporal_overlap_count=0`。所有 series 只按 Scheduled Start 归属一个 split，同一 series 的小局不得另行拆分。

## 4. 描述性覆盖

### 4.1 Region

| Split | Region counts |
|---|---|
| Train | EMEA 613; Korea 243; China 136; Americas 114; International 101; Asia Pacific 73; Asia 1 |
| Validation | EMEA 216; Korea 77; China 70; Asia Pacific 37; North America 19; Brazil 11 |
| Calibration | EMEA 307; Korea 169; North America 92; Asia Pacific 60; China 49; Brazil 49; International 17 |
| Final Test | EMEA 303; Korea 123; Asia Pacific 93; North America 57; China 47; Brazil 43; International 35 |

`Americas` 与 `North America` 是上游时间化 Competition identity 的原始 region value，本任务不为美观而事后合并。由此形成的 unsupported `Region×BO` cell 必须在 M3R-005 中显式报告并回退 Elo。

### 4.2 Patch

| Split | Patch counts |
|---|---|
| Train | 25.13 124; 25.14 190; 25.15 224; 25.16 187; 25.17 149; 25.18 56; 25.19 114; 25.2 86; 25.21 39; 25.22 47; 25.23 29; 25.24 36 |
| Validation | 25.24 3; 26.01 128; 26.02 142; 26.03 124; 26.04 33 |
| Calibration | 26.03 12; 26.04 60; 26.05 112; 26.06 116; 26.07 239; 26.08 204 |
| Final Test | 26.08 39; 26.09 281; 26.1 230; 26.11 124; 26.12 22; 26.13 5 |

这里保留上游字符串 `25.2`、`26.1`，不擅自解释为 `25.20`、`26.10`。P0 模型不使用 Patch feature；若 P1 后续需要 Patch adaptation，必须先独立解决版本标识规范化并保留原始值。

### 4.3 Missingness 与来源时间

| Split | Prior history unavailable | Same-Patch unavailable | Source-time violations/checks |
|---|---:|---:|---:|
| Train | 45 / 2,562 team-sides (1.76%) | 993 / 2,562 (38.76%) | 0 / 15,723 |
| Validation | 33 / 860 (3.84%) | 367 / 860 (42.67%) | 0 / 5,121 |
| Calibration | 50 / 1,486 (3.36%) | 549 / 1,486 (36.95%) | 0 / 9,054 |
| Final Test | 38 / 1,402 (2.71%) | 520 / 1,402 (37.09%) | 0 / 8,584 |

`unavailable` 不编码为 0% 胜率。Final 的 Region、BO、Patch 和 missingness 只作预注册要求的无标签描述；不得据此选择 feature、参数、模型或 calibration。

## 5. 可重复验证

聚合报告由 `research/m3r004_split_coverage.py` 从 Feature Snapshot 与 sealed split 重新计算。该工具验证 feature hash、recovery 零重叠、development membership/window、Final count/commitment 和 source time；输出只包含 aggregate，不含 `series_ids` 或 label。

```powershell
.\.venv\Scripts\python.exe research/m3r004_split_coverage.py `
  --features data/processed/lol-prematch-features/2026-08-13.f42324d.m3r003-features-v1/prematch-feature-snapshots.json `
  --split data/processed/lol-temporal-splits/2026-08-14.3d155d3.m3r004-split-v1/temporal-split-manifest.json
```

已通过：Rust temporal split 8 项测试、Python aggregate-only seal 测试、实际 3,155 行重放、Dataset Manifest Rust validator、PowerShell parser、固定版本 `ruff` 和 `git diff --check`。

## 6. 对后续开发计划的约束

- M3R-005 只能消费 Train / Validation / Calibration 的 2,454 个公开 Development IDs。
- Final Test 的 701 个成员在模型、特征、参数、fallback、calibration 与 Gate 阈值冻结前不得 release。
- Calibration 与 Final 的 BO5 占比分别为 11.17% 和 28.53%，赛制构成漂移仍然存在；P0 必须用逐局概率的 BO3/BO5 DP，而不是把 BO5 当作可自由拟合的单一 dummy。
- Final 存在新的时间、Patch 和 Region 组合是预期的时间外推，不允许用 post-hoc weighting 或删样本修饰主结果；固定 composition 只作诊断。
- M3R-004 完成不等于恢复模型有效，也不解除 M4、策略、PnL 或执行阻塞。
