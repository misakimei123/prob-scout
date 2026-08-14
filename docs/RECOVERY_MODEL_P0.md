# M3R-005 P0 Recovery Model

> 状态：Completed — `failed_public_stability_stop_before_final`
>
> 固定日期：2026-08-14
>
> 范围：仅使用 2,454 条公开 Development；701 条新 Final Test 未 release、未推导成员、未计算指标。

## 1. 结论先行

P0 在 1,173 条 Walk-forward evaluation 的自然构成汇总上略优于生成式 Elo，但优势集中在第三个窗口，缺乏跨时间稳定性：Fold 1、2 的 Brier 和 Log Loss 均劣化，Fold 4 仅 Brier 微弱改善而 Log Loss 劣化；固定共同 `Region×BO` 构成后，仍有 3/4 folds 同时劣于 Elo。因此不能把总体 `-0.00115` Brier / `-0.00180` Log Loss 解释为可进入新 Final 的稳健优势。

当前裁决是停止在公开 Development：P0 证明对手质量 residual 存在局部信息，但没有满足“多个时间窗口方向一致”的预注册要求。不得进入 M3R-006，不得 release 701 条新 Final。下一步只能先决定是否授权有明确增量证据的 P1，或结束恢复模型路线。

## 2. 数据事实对设计的修正

现有 `ScoreboardGames` evidence 没有逐局 winner，只提供 game number、时间、Patch 和时长；Series Result 只提供最终比分。系统不能据此伪造小局胜负顺序。

P0 因而采用 `series_atomic_game_count_batch`：

1. 每个 series 在 `T-15m` 用当时 rating 生成 game-level Elo probability；
2. 由固定 DP 将 game probability 映射为 BO3/BO5 series probability；
3. 只有 series 的真实 `completed_at_utc <= target T-15m` 时，其总比分才进入 history；
4. Elo 按 `K × (team_1_game_wins - game_count × pregame_probability)` 做零和 batch update。

这保留了小局数量的信息密度和严格 completed-before-cutoff 语义，但不声称恢复了不可观察的逐局顺序。若未来要做真正 per-game sequential Elo，必须先扩展带 winner 与精确 `available_at` 的 Game Result evidence。

## 3. 两速 Feature Lab

Rust 继续负责 identity、result/cutoff、split、seal 与 Dataset Manifest；P0 实验列保留在 `research/model008_recovery_model.py`，没有新增 SQLite migration 或为每列增加 Rust struct。

模型矩阵使用 7 个双方差值：

- `opponent_adjusted_residual_30d_diff`：按小局数加权、30 天半衰期的 `actual game win share - pregame Elo p`；
- `opponent_adjusted_residual_90d_diff`：同口径 90 天半衰期 residual；
- `strength_of_schedule_90d_diff`：90 天半衰期的对手 pregame rating，中心化到 1500、按 400 缩放；
- `games_7d_diff`、`games_14d_diff`：cutoff 前已完成的小局负荷；
- `rest_days_capped_diff`：距最后完成 series 的天数，30 天封顶并缩放；
- `log_history_games_diff`：累计可用小局数的 `log1p`。

Availability 不再作为一个在 supported train rows 中恒为 0 的冗余系数；它通过每队 `history_supported` 审计和预测 fallback 实现。每个 team-feature 仍输出 `source_max_at/input_count/status`。本次 materialization 为 2,454 个 model rows、39,264 个 audit rows，source-time violation 为 0。

## 4. 模型合同

- Elo 初值/尺度/每小局 K：`1500 / 400 / 20`；恢复 cohort 边界冷启动，不消费 retired Final label。
- Residual model：game Elo logit fixed offset；7 个标准化 residual 和 intercept 由 series Bernoulli likelihood 拟合。
- Series link：BO3 `p²(3-2p)`；BO5 `p³(10-15p+6p²)`，与确定性 DP 等价。
- Regularization：固定 L2 penalty 10；`scipy.optimize.minimize(method="L-BFGS-B")`，没有基于窗口指标调参。
- Fallback：任一队少于 6 个历史小局，或训练期该 `Region×BO` cell 少于 30 个 series，直接使用生成式 Elo series probability。
- Walk-forward：按 2026-01、02、03、04 四个自然月评估；每个 fold 只在此前 Development 重新拟合 residual coefficients，Elo 只吸收各 cutoff 前已完成结果。
- Calibration：未应用；P0 直接评估 raw generative probability。

## 5. Walk-forward 结果

Delta 定义为 `offset residual - Elo`，负值更好。

| Fold | Evaluation | Fit rows | Fallback | Brier delta | Log Loss delta |
|---|---:|---:|---:|---:|---:|
| 2026-01 | 195 | 795 | 45 | +0.00091096 | +0.00141658 |
| 2026-02 | 235 | 945 | 65 | +0.00131633 | +0.00454702 |
| 2026-03 | 259 | 1,156 | 93 | -0.00640617 | -0.01580802 |
| 2026-04 | 484 | 1,345 | 70 | -0.00036532 | +0.00131078 |
| Pooled natural composition | 1,173 | — | 273 | -0.00115008 | -0.00180314 |

Pooled Elo / model Brier 为 `0.21663852 / 0.21548844`，Log Loss 为 `0.62264008 / 0.62083694`。273 次 fallback 中，team history-only 149、cell-only 99、两者同时 25。

## 6. 固定 Region×BO 构成与反例

四个 fold 共同支持且实际出现的 cells 只有 `Asia Pacific|BO3`、`EMEA|BO3`、`EMEA|BO5`、`Korea|BO3`；参考权重分别为 10.01%、52.63%、11.23%、26.13%。固定该构成后：

| Fold | Common-cell coverage | Brier delta | Log Loss delta |
|---|---:|---:|---:|
| 2026-01 | 135 / 195 (69.23%) | +0.00169860 | +0.00379295 |
| 2026-02 | 159 / 235 (67.66%) | +0.00654672 | +0.01780064 |
| 2026-03 | 175 / 259 (67.57%) | -0.00708181 | -0.01836473 |
| 2026-04 | 350 / 484 (72.31%) | +0.00158582 | +0.00371335 |

这排除了“只因 Region/BO 构成变化才出现不稳定”的充分解释。主要可复核反例包括：2026-02 Korea（28 场，两个指标劣化）、2026-04 China BO3（42 场，两个指标劣化）、2026-04 BO5（12 场，两个指标劣化）。2–8 场的极小 cell 虽也有大幅波动，只作为 evidence gap，不作模型判断。

## 7. 产物与验证

- Config：`research/model008_recovery_config.json`
- Builder：`research/build_recovery_model.ps1`
- Artifact：`artifacts/models/recovery-model/2026-08-14.3d155d3.m3r005-p0-v4/recovery-model-artifact.json`
- Artifact SHA-256：`f4e4892ca5daffd5edb1bfc2b785cf74cb8bb8fcc26860c2ab058c8d441a2144`
- Artifact Manifest SHA-256：`da65cd374b08ec87a095c5268242c7b0ee94c0010bbb208000b1475daf5bddec`

相同输入双构建 hash 一致。定向测试覆盖 BO3/BO5 精确概率、未完成 series 不得进入后续 `T-15m`、series 完成后的 game-count Elo update、offset optimizer/open probability 和 aggregate-only Final seal。Artifact 保留全部 fold、Region、BO、`Region×BO`、fallback、counterexample、参数、model matrix 和 audit rows；Final 只保留 count/commitment/status。

## 8. 开发决策

| 路径 | 开发成本 | 预期信号增量 | 过拟合风险 | 当前优先级 |
|---|---|---|---|---|
| 直接 release 新 Final | 低 | 无新增信息 | 极高：用不稳定候选消耗唯一 holdout | 禁止 |
| P1a：补真实 Game Result / roster availability evidence，再做 player/lineup continuity | 高 | 中高，可能修复换阵与真实逐局更新盲点 | 中；需严格来源秒级时间 | 候选 1，需另行授权 |
| P1b：对现有 7 维 residual 做更多参数/窗口搜索 | 低 | 低 | 高：已看到四窗结果后易多重试验过拟合 | 不推荐 |
| 停止统计恢复，保留生成式 Elo | 低 | 不新增 alpha | 低；科学上最稳健 | 当前默认 |

在没有新增原子证据前，推荐保留生成式 Elo 作为预测基准并停止消耗新 Final。P1 不能只是对当前 half-life、K、L2 或阈值做网格搜索。
