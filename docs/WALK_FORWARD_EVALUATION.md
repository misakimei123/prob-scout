# MODEL-006 Walk-forward 评估

更新日期：2026-08-13

## 1. 评估合同

MODEL-006 只消费公开 Development 的 1,422 场 Series/Feature，不读取或 release sealed final test 成员。评估使用三个 expanding folds，每个 fold 严格按以下顺序排列：

```text
expanding train -> disjoint calibration -> later evaluation
```

- Constant Baseline 只使用本 fold expanding train 的 label prior。
- Elo 使用固定 `1500/400/K20`，按既有 MODEL-002 合同逐场先预测后更新。
- Raw Statistical 每个 fold 只在 expanding train 重新拟合固定的 `StandardScaler + LogisticRegression`。
- Calibrated Statistical 只用紧邻 evaluation 之前且不重叠的 calibration window 拟合 sigmoid。
- 三个 evaluation window 互不重叠，所有窗口和分段均完整报告。

Market Baseline 没有进入交叉模型指标，因为它只有不同时间范围的 16 场 linked-only Grade C 母体；把它与当前 959 场混合比较会制造样本选择偏差。

## 2. 时间窗口

| Fold | Expanding train | Calibration | Evaluation | Train / Cal / Eval |
|---|---|---|---|---:|
| 1 | 2025-01-12–2025-02-23 | 2025-02-23–2025-03-16 | 2025-03-16–2025-04-07 | 325 / 138 / 211 |
| 2 | 2025-01-12–2025-03-16 | 2025-03-16–2025-04-07 | 2025-04-07–2025-04-28 | 463 / 211 / 364 |
| 3 | 2025-01-12–2025-04-07 | 2025-04-07–2025-04-28 | 2025-04-28–2025-05-19 | 674 / 364 / 384 |

共评估 959 场；初始训练和首个校准窗口的 463 场不作为 out-of-time evaluation。最后一个 evaluation 截止于 final test 开始时点 `2025-05-19T00:00:00Z`。

## 3. 整体结果

| Model | Brier | Log Loss |
|---|---:|---:|
| Constant | 0.2467194805 | 0.6865756827 |
| Elo | 0.2264871457 | 0.6447592740 |
| Raw Statistical | **0.2240006321** | **0.6393848851** |
| Calibrated Statistical | 0.2241384269 | 0.6399372269 |

相对 Elo：

- Raw Statistical：Brier `-0.0024865136`，Log Loss `-0.0053743890`；
- Calibrated Statistical：Brier `-0.0023487188`，Log Loss `-0.0048220471`。

整体 calibrated minus raw 为 Brier `+0.0001377948`、Log Loss `+0.0005523419`，即本次 Walk-forward 中校准后略差于 raw。

## 4. 各时间窗口

| Fold | Elo Brier / Log Loss | Raw Brier / Log Loss | Calibrated Brier / Log Loss |
|---|---:|---:|---:|
| 1 | 0.2434276134 / 0.6798728806 | 0.2417174221 / 0.6781239483 | **0.2381761741 / 0.6701187504** |
| 2 | 0.2300244213 / 0.6520263614 | **0.2295458669 / 0.6504675049** | 0.2298287645 / 0.6516405782 |
| 3 | 0.2138256690 / 0.6185764856 | **0.2090092046 / 0.6075931768** | 0.2110310115 / 0.6122593069 |

Raw Statistical 在三个 fold 的 Brier 和 Log Loss 都低于 Elo，但优势幅度不同。Sigmoid 只在 fold 1 改善 raw，在 fold 2/3 分别使 Brier 增加 `0.0002828976`、`0.0020218068`。因此“校准稳定改善概率”的命题不被当前 Walk-forward 证据支持。

## 5. 赛区分段

| Region | N | Elo Brier | Raw Brier | Calibrated Brier | 主要反例 |
|---|---:|---:|---:|---:|---|
| Americas | 168 | **0.2351978498** | 0.2452187584 | 0.2405800158 | 两个统计版本均差于 Elo |
| Asia Pacific | 66 | 0.2091868346 | **0.2038154395** | 0.2059702969 | calibrated 差于 raw |
| China | 100 | **0.2259600947** | 0.2398174160 | 0.2364076645 | 两个统计版本均差于 Elo |
| EMEA | 354 | 0.2382713114 | 0.2345676277 | **0.2326263671** | — |
| International | 21 | 0.2486517651 | 0.2365168174 | **0.2358341612** | 小样本 `<30` |
| Korea | 250 | 0.2068634482 | **0.1927300031** | 0.1999770054 | calibrated 差于 raw |

Americas 和 China 是明确反方证据；International 只有 21 场，artifact 标记 `small_sample_warning=true`。不能用整体优势掩盖赛区异质性。

## 6. 赛制分段

| Best of | N | Elo Brier / Log Loss | Raw Brier / Log Loss | Calibrated Brier / Log Loss |
|---|---:|---:|---:|---:|
| BO3 | 887 | 0.2252858659 / 0.6422619179 | **0.2220850253 / 0.6353707779** | 0.2224679601 / 0.6365011678 |
| BO5 | 72 | **0.2412862444 / 0.6755253144** | 0.2475998439 / 0.6888364558 | 0.2447176501 / 0.6822675667 |

统计模型的整体优势主要来自 BO3；BO5 上 raw/calibrated 都差于 Elo。72 场虽高于预设 30 场小样本标记线，但仍不足以支持稳定性外推。

## 7. 构建结果和边界

固定版本：`2026-08-13.e87d978.model006-v2`

- 三个 fold、959 个 evaluation series 的 membership 均写入 SHA-256 commitment。
- 输出整体、逐 fold、赛区、BO3/BO5 的全部模型指标和 10-bin raw/calibrated calibration curve。
- 相同输入双构建 hash 一致。
- 356 条 final test 继续 sealed；artifact 不包含其 ID、label、prediction 或指标。
- MODEL-006 不作 Gate 1 决策；是否继续、回退校准或停止由 MODEL-007 独立完成。

Artifact SHA-256：`bd08f5694d8c81b33b18af336614a29488e2ba7015c7274fadc62d74f25c9c4f`

配置 SHA-256：`85f2224878f439f0731c28512552ac0aea94df83f6632538cc9ad0df642ac4fb`
