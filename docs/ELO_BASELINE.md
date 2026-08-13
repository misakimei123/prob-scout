# MODEL-002 Elo Baseline

更新日期：2026-08-13

## 1. 合同

Elo Baseline 只消费 development Temporal Split 中公开的纯 Series Result，使用全局 Canonical Team rating pool。配置固定为：

- initial rating：`1500`
- rating scale：`400`
- K-factor：`20`
- update unit：一场完整 BO3/BO5 Series Result
- positive label：`team_1_win`

所有比赛按 `(scheduled_start_utc, series_id)` 稳定排序。每场必须先使用 pre-match ratings 计算概率，再根据最终胜者更新双方 rating；当前赛果不能影响自身预测。首次参赛队伍使用 1500，跨赛区或 International 比赛沿用同一 Canonical Team 已累积的全局 rating，不按赛区重置或增加未经验证的 region offset。

同一开赛时刻的不同队伍比赛可按 `series_id` 确定性输出，因为结果之间不存在共享 rating；如果同一 Canonical Team 在完全相同的开赛时刻出现两场比赛，构建 fail closed，不人为推断先后关系。

## 2. 构建入口

```powershell
./research/build_elo_baseline.ps1 -Version <new-immutable-version>
```

构建脚本校验 Series Result/Temporal Split 的 Dataset Manifest v1 与 output hash，使用 `uv run --frozen` 连续构建两次并比较 artifact SHA-256。输出位于 Git 忽略的 `artifacts/models/elo-baseline/<version>/`。

artifact 保存：

- Elo 配置及其规范 JSON SHA-256；
- 每场 development prediction、pre-match ratings、首次参赛标记和实际 label；
- train/validation/calibration 的 Brier 与 Log Loss；
- calibration 结束后的 terminal ratings；
- sealed final-test count、commitment、access policy 与未来 release 要求。

## 3. 真实构建结果

固定版本：`2026-08-13.68be155.model002-v2`

| 项目 | 结果 |
|---|---:|
| Development series | 1,422 |
| Unique Canonical Teams | 319 |
| First-time team sides | 319 |
| Train Brier / Log Loss | 0.2422573700 / 0.6775746090 |
| Validation Brier / Log Loss | 0.2427027093 / 0.6784341773 |
| Calibration Brier / Log Loss | 0.2217084843 / 0.6348542326 |
| Sealed final test | 356；未评估 |

Artifact SHA-256：`49e71bdbc29b19f964cdd4f7db08f7f46d6b21eff981f566efd2541590255a40`

配置 SHA-256：`604873b8cbecd7d7a7cdacb799fec6ba6090deb8ecae3f82647834dbe1a7c9fe`

相同输入双构建 hash 一致；1,422 条 prediction 与 319 个 terminal rating 完整生成。artifact 不包含 final-test IDs、labels 或指标。

## 4. 边界

- validation/calibration 中某场比赛可使用其之前已完成的 development 赛果更新，但绝不使用自身或未来赛果；这是 chronological online baseline，不是固定 train-only classifier。
- 当前固定参数是 MODEL-002 baseline 合同，不是通过 validation 搜索出来的最优 Elo 参数。
- development 指标不能证明 final-test 表现、跨年度稳健性或 M3 Gate 通过。
- Elo 只使用赛果，不读取 Feature Snapshot、Market Resolution Link、价格或交易数据。
- final test 继续保持 sealed；MODEL-002 不执行 release。
