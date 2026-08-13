# MODEL-004 第一版统计模型

更新日期：2026-08-13

## 1. 模型合同

第一版统计模型使用 scikit-learn `Pipeline(StandardScaler, LogisticRegression)`。模型只在 train split 的 325 场公开 Development Series 上拟合；validation 与 calibration 只调用 `predict_proba` 评估 raw probability，不参与 scaler、系数或截距拟合。随机种子固定为 `20260813`，`liblinear` solver、L2 regularization、`C=1.0`、`max_iter=1000` 和 tolerance `1e-8` 均写入 artifact metadata。

本任务不执行概率校准。输出明确标记 `raw_uncalibrated`，MODEL-005 才能消费 calibration split 拟合独立校准器。

## 2. 可解释特征

只使用固定 `T-15m` Feature Snapshot 中双方赛前 team form 的差值，正方向始终为 `team_1 - team_2`：

1. prior series win rate；
2. prior game win rate；
3. same-Patch series win rate；
4. `log1p` prior series count；
5. `log1p` prior game count；
6. `log1p` same-Patch series count；
7. `log1p` rest minutes；
8. prior-history availability；
9. same-Patch-history availability；
10. BO5 indicator。

Feature Snapshot 的队伍按显式 `team_id` 与 Series Result 对齐，不依赖 JSON 数组位置。无历史胜率在模型矩阵中使用中性值 0.5，同时保留 availability 差值；这只是缺失值编码，不把 unavailable 解释成真实 50% 胜率。任一 source time 晚于 snapshot、snapshot 不等于 `Scheduled Start - 15m`、分子分母矛盾、赛事/队伍/BO/Patch 不一致或出现目标/市场结算字段时均 fail closed。

## 3. 构建入口

```powershell
./research/build_statistical_model.ps1 -Version <new-immutable-version>
```

构建脚本固定消费同一 HIST-010 批次的 Series Result、Feature Snapshot 和 Temporal Split，分别校验 Dataset Manifest v1 与 output hash。相同输入连续生成两次，artifact SHA-256 不一致时不落盘。

## 4. 真实构建结果

固定版本：`2026-08-13.7605cdd.model004-v2`

| Split | Series | Brier | Log Loss | Mean raw probability |
|---|---:|---:|---:|---:|
| train | 325 | 0.2211313304 | 0.6345254046 | 0.5227560802 |
| validation | 349 | 0.2383272878 | 0.6712635152 | 0.4979830773 |
| calibration | 748 | 0.2193676117 | 0.6294449007 | 0.5150620366 |

- 训练在 7 次 optimizer iteration 后收敛。
- 1,422 条 Development prediction 的概率范围为 `0.0885007249`–`0.9283515588`。
- 356 条 final test 继续 sealed；artifact 不包含其 ID、label 或指标。
- 相同输入双构建 hash 一致。

Artifact SHA-256：`7035396395c726232fe07e5b119b5d7c4cf0b39d60fef2fa2a2a77a789ba2611`

配置 SHA-256：`27dc7e6c93cf21109d51b5255306d12f7834ea1d3f150b9e57f7a9135aae5629`

## 5. 解释边界

- LogisticRegression 系数描述控制其他输入后的线性 log-odds 关系，不自动构成因果结论。
- raw coefficients、StandardScaler 的 train mean/scale 和 standardized coefficients 均写入 artifact，便于重放和审查。
- 当前结果未经过 MODEL-005 校准，也未执行 MODEL-006 Walk-forward；不能据此宣布 Gate 1 通过。
- 单一年份、Same-Patch unavailable 和来源事后修订风险仍继承自 M2 数据质量报告。
