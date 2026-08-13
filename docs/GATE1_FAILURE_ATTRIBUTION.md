# M3R-001 Gate 1 失败归因

更新日期：2026-08-13

## 1. 范围与治理结论

本任务只消费不可变 MODEL-004、MODEL-006 和 MODEL-007 artifacts，不重新读取模型训练输入、不重新训练、不重新选择候选，也不作第二次 Gate 裁决。

已释放的 356 场 Final Test 从本任务起永久标记为：

```text
retired_diagnostic_evidence_never_independent_again
```

它可以用于解释已发生的失败，但禁止用于后续 feature、model、hyperparameter、calibration 选择，也不得成为未来 Gate 的 Final Test。

## 2. 完全相同冻结模型的公开重放

MODEL-006 的 fold 2/3 使用 expanding train，最终 fold 的 train 为 674 场；MODEL-007 实际冻结并发布的是原始 MODEL-004 325 场训练模型。两者存在已确认的评估—部署训练协议差异。

为排除该差异对公开结论的干扰，本任务使用 MODEL-004 artifact 已保存的 raw probability，在 MODEL-006 相同的 959 个公开 evaluation IDs 上重放完全相同的冻结模型：

| Public evaluation | Fixed raw Brier - Elo | Fixed raw Log Loss - Elo |
|---|---:|---:|
| Fold 1 | -0.0017101913 | -0.0017489323 |
| Fold 2 | -0.0007309062 | -0.0008473199 |
| Fold 3 | -0.0038669865 | -0.0097337391 |
| Overall | -0.0022021095 | -0.0046039676 |

结论：完全相同的冻结候选在公开三个 fold 上仍全部略优于 Elo。Expanding/frozen 差异需要在未来 Gate 中消除，但它不足以解释 Final Test 的性能符号反转，也不能据此推断“多训练 349 场即可修复”。

## 3. 样本构成变化

最显著的可观测变化是 BO 分布：

| Cohort | BO3 | BO5 |
|---|---:|---:|
| Public Walk-forward | 92.49% | 7.51% |
| Retired Final | 52.81% | 47.19% |

冻结 raw 模型在公开 BO5 上已经明显弱于 Elo：Brier/Log Loss delta 为 `+0.0183495612/+0.0387574480`。因此 Final Test 的 BO5 权重大幅上升确实会机械性放大失败。

按共同 `Region×BO` cells 做描述性 Oaxaca-style 分解：

| Component | Brier delta | Log Loss delta |
|---|---:|---:|
| Public fixed raw - Elo | -0.0022021095 | -0.0046039676 |
| Composition effect | +0.0077955435 | +0.0157653847 |
| Within-cell residual | +0.0084671032 | +0.0200444911 |

构成变化和共同 cell 内退化都具有实质影响，不能只归因于 BO5 比例。该分解是描述性的；within-cell residual 同时包含时间变化、模型陈旧、对手组合和未观测因素，不构成因果识别。

Final 另有 18 场 `China|BO5`，占 `5.06%`，公开 Walk-forward 中没有同 cell 参照。本任务不对该 cell 外推公开表现，只单独记录其实际 raw 相对 Elo Brier/Log Loss delta `+0.0074565520/+0.0156746035`。

## 4. 分段内时间反转与预测分歧

若失败只来自构成变化，同一 `Region×BO` 内的 raw/Elo 关系应大致保持。实际存在明确反转：

- `EMEA|BO3`：公开 Brier delta `-0.0084599513`，Final `+0.0248631202`；
- `Korea|BO3`：公开 `-0.0150094079`，Final `+0.0018284075`；
- `China|BO3`：公开 `+0.0199356174`，Final `-0.0259204079`，方向相反但 Final 仅 8 场。

Final 从 2025-05-19 起按 7 天窗口切分，前六个样本数不少于 20 的窗口 raw 均劣于 Elo；最后一个只有 3 场的窗口 raw 更好，不足以推翻持续退化证据。

按 0.5 hard-class 比较：

| Category | Series | 对总体 Brier delta 的贡献 |
|---|---:|---:|
| Both correct | 167 | -0.0080937128 |
| Both wrong | 99 | +0.0169913985 |
| Elo only correct | 49 | +0.0169974649 |
| Raw only correct | 41 | -0.0121685228 |

Raw 与 Elo 的平均概率差为 `0.07899`，P90 为 `0.18991`。失败不是仅由少数 hard-class 翻转产生；两者同时判断错误时，raw 的概率幅度也贡献了显著额外 Brier 损失。

## 5. 可支持与不可支持的结论

当前证据支持：

- Gate 1 失败是真实且跨多个 Final 时间块存在；
- 精确冻结候选的公开优势很小，无法外推到更晚窗口；
- BO5/赛区构成变化解释部分反转，但共同分段内仍发生明显退化；
- Walk-forward procedure 与最终冻结模型的训练协议不一致，未来必须统一；
- `China|BO5` 是公开阶段未覆盖的明确 blind spot。

当前证据不支持：

- 某个具体 feature 是失败根因；
- 增加旧训练数据必然修复失败；
- 按赛区或 BO 单独建模一定能够泛化；
- 在旧 Final 上验证任何改进版本；
- 绕过 Gate 进入策略或 PnL。

## 6. 下一任务

只授权 `M3R-002` 数据扩展：建立时间严格不早于 `2025-07-01T00:00:00Z`、与旧 corpus 零重叠的新 source-identity candidate corpus。新数据须覆盖足够 BO3/BO5、Region 和 Patch 组合，并为后续全新 sealed Final Test 保留独立时间窗口。

本任务 artifact SHA-256：`ba126c4ea192f4078f8795646796fa37cf5a2503a9f0cd7a89c59cd7e543271c`
