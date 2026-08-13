# MODEL-005 概率校准

更新日期：2026-08-13

## 1. 校准合同

MODEL-005 固定消费不可变的 MODEL-004 v2 artifact，不重新读取特征或重新拟合底层 LogisticRegression。实现使用 scikit-learn `CalibratedClassifierCV(method="sigmoid")`，并以 `FrozenEstimator` 包装只返回 MODEL-004 raw probability 的 identity classifier；因此底层信号没有可训练参数，只有公开 calibration split 的 748 个 label 参与 sigmoid 映射拟合。

校准方法预注册为 sigmoid，不在同一 calibration split 上比较并挑选 sigmoid/isotonic。原因是 sigmoid 只有 slope/intercept 两个参数、映射单调且可直接序列化；isotonic 参数更多，在当前样本规模下更容易把 calibration split 的噪声拟合进映射。

## 2. 输出边界

Calibration artifact 同时保存：

- MODEL-004 artifact/manifest/config SHA-256；
- sigmoid 配置、拟合成员 SHA-256、`a`/`b` 参数和可独立重放的映射公式；
- 1,422 条公开 Development series 的 raw/calibrated probability；
- calibration split 上校准前后的 Brier、Log Loss 和 10 个 quantile-bin calibration curve；
- final test 的 count、membership commitment 和 seal，不包含 final test ID、label、prediction 或指标。

train/validation 的 calibrated probability 只是映射重放参考，因为该映射使用了时间上更晚的 calibration label；不得将它们解释为 out-of-time 评估。calibration split 指标同样是 in-sample fit diagnostic，不是 Gate 1 证据。无偏的多窗口比较属于 MODEL-006。

## 3. 构建入口

```powershell
./research/build_probability_calibration.ps1 -Version <new-immutable-version>
```

构建脚本固定消费 `2026-08-13.7605cdd.model004-v2`，验证其 artifact manifest 与 SHA-256。相同输入连续生成两次，artifact SHA-256 不一致时拒绝落盘；已有版本目录禁止覆盖。

## 4. 真实构建结果

固定版本：`2026-08-13.e9ed531.model005-v2`

拟合数据：calibration split 748 场，其中 `team_1_win=1` 为 424 场。

拟合映射：

```text
calibrated = expit(-(-4.7999993295 * raw_probability + 2.1715170657))
```

| Calibration fit diagnostic | Raw | Calibrated | Calibrated - Raw |
|---|---:|---:|---:|
| Brier | 0.2193676117 | 0.2161185633 | -0.0032490484 |
| Log Loss | 0.6294449007 | 0.6221717139 | -0.0072731868 |
| Mean `P(team_1_win)` | 0.5150620366 | 0.5668429957 | +0.0517809591 |

- 10 个 quantile bins 均返回有效 calibration curve point。
- 1,422 条 Development calibrated probability 范围为 `0.1484621830`–`0.9075910280`。
- 356 条 final test 继续 sealed，未读取成员或计算指标。
- 相同输入双构建 hash 一致。

Artifact SHA-256：`3ba241cbcbfbd397591daf7d8f0f7cefb905c46f5940928f4b0692aa95ea16df`

配置 SHA-256：`4b65c38353b43a6c657a2dcd50781e122d4b905e9d09b695ddf28f969bbe3c6b`

## 5. 解释边界

- 负的 Brier/Log Loss delta 只说明校准器更贴合其拟合 split，不能当作未来窗口收益。
- 当前 calibrated probability 仍不构成交易建议，也不包含 Market Quote、ask、depth、fee 或可成交 PnL 语义。
- MODEL-006 必须按时间做 Walk-forward，并完整报告模型与基准在所有窗口的表现；不能只展示最好窗口。
- MODEL-007 冻结模型、配置、校准和评估代码前，不得 release final test。
