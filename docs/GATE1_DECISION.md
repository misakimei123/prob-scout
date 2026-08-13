# MODEL-007 Gate 1 决策

更新日期：2026-08-13

## 1. 冻结与候选选择

Final Test release 前已根据公开 Development Walk-forward 固定选择 `raw_statistical`：raw 在 3/3 个公开 evaluation fold 的 Brier 与 Log Loss 均略优于 Elo，而 sigmoid calibration 只改善 fold 1，在 fold 2、fold 3 和整体上均恶化。因此 MODEL-007 在 release 前回退 sigmoid，Final Test 不计算 calibrated probability，也不允许看过 Final Test 后重新选择模型。

冻结版本：`2026-08-13.e87d978.model007-v1`

- MODEL-004 raw artifact：`7035396395c726232fe07e5b119b5d7c4cf0b39d60fef2fa2a2a77a789ba2611`
- raw model config：`27dc7e6c93cf21109d51b5255306d12f7834ea1d3f150b9e57f7a9135aae5629`
- MODEL-005 calibration artifact：`3ba241cbcbfbd397591daf7d8f0f7cefb905c46f5940928f4b0692aa95ea16df`
- MODEL-006 Walk-forward artifact：`bd08f5694d8c81b33b18af336614a29488e2ba7015c7274fadc62d74f25c9c4f`
- Gate config：`3a884f7c766fe58a3975c9f13e3a052f509fff99004f0241aec9f65f5307098d`
- evaluation code bundle：`6ff9589ffc11aa0c71c23320797440d9cc566b56409d38d1d2c9332c19256e81`

Rust release 重新从完整 Series Result 计算 356 场 Final Test 成员，并核对原 seal commitment `c5b7295b8363bc62c4cbf8d1c0edc798179fa09ad6634060f5207b1397a39f1d`。成功 artifact 标记 `released_and_evaluated_once`，相同 version 目录和 released manifest 均禁止覆盖。

在成功主评估前出现两次 fail-closed 中止：一次是 RFC3339 小数秒文本规范化差异，一次是 Python 误把 Rust 的 `(scheduled_start_utc, series_id)` commitment 当作纯 ID hash。两次都发生在 label、feature、prediction 和 metric 读取/计算之前，未生成结果 artifact；修正合同并补测试后才执行唯一一次成功主评估。

## 2. Final Test 结果

Final Test 共 356 场，`team_1_win=1` 为 194 场。

| Model | Brier | Log Loss | Mean probability |
|---|---:|---:|---:|
| Constant | 0.2484582142 | 0.6900617955 | 0.5230769231 |
| Elo | **0.2363180786** | **0.6648857090** | 0.5092055201 |
| Raw Statistical | 0.2500447064 | 0.6953063265 | 0.4967894556 |

Raw 相对 Elo：

- Brier `+0.0137266278`，超过预注册最大退化 `+0.01`；
- Log Loss `+0.0304206175`，超过预注册最大退化 `+0.02`。

公开 Walk-forward 959 场与 Final Test 合并后，raw 相对 Elo 的 Brier/Log Loss 仍分别为 `+0.0019027475/+0.0043161223`，两个综合非劣检查均失败。四个顺序窗口中 raw 在前三个公开窗口占优、在 Final Test 窗口显著落后，因此公开小幅优势没有外推到冻结窗口。

## 3. 分段与校准证据

Final Test 上 raw 在 Americas、Asia Pacific、EMEA、Korea、BO3 和 BO5 均劣于 Elo；仅 China 与 International 更好，但样本分别只有 26 和 8，均触发 `<30` 小样本警告。主要反例：

| Segment | N | Raw Brier - Elo | Raw Log Loss - Elo |
|---|---:|---:|---:|
| Americas | 48 | +0.0201880792 | +0.0410841682 |
| EMEA | 164 | +0.0212224054 | +0.0486570976 |
| BO3 | 188 | +0.0125161914 | +0.0269451526 |
| BO5 | 168 | +0.0150811638 | +0.0343098283 |

Raw 的总体 mean predicted-class confidence 为 `0.5930984225`，classification accuracy 为 `0.5842696629`，差值 `+0.0088287595`，未超过 `+0.05` 系统性过度自信阈值。`>=0.8` confidence 只有 8 场，低于 30 场门槛，其较大 gap 不参与硬裁决。校准门禁通过不能抵消相对 Elo 的性能失败。

## 4. Gate 1 结论

结论：`failed_stop_modeling`。

- sigmoid calibration 已在 release 前回退；
- raw statistical 未通过 Final Test 灾难性退化和综合非劣保护线；
- `BACK-001` 及后续 M4 策略/PnL 开发不被授权；
- 不允许在看过本次 Final Test 后修改模型并用同一 356 场重新裁决；
- 若要恢复模型路线，必须建立新的独立 out-of-sample cohort、产生全新版本和 seal，并把本次失败结果永久保留为历史证据。

成功 Gate artifact SHA-256：`8380bb33219277e8404dd9b07c28ecda00aa19e27d1d09cad96f39ffd406af37`
