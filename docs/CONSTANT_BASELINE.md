# MODEL-001 Constant Baseline

更新日期：2026-08-13

## 1. 合同

Constant Baseline 使用 scikit-learn `DummyClassifier(strategy="prior")`，正类固定为 `team_1_win`。它只从 train split 的 Series Result label 拟合一个总体先验概率，不读取 Feature Snapshot、Market Resolution Link 或市场价格，也不按队伍、赛区、Patch 或 BO 类型分段。

validation 与 calibration 只用于评估同一个冻结概率，不参与拟合。development manifest 中一旦出现 final-test IDs，或 Series Result 行数不等于 development 数量加 sealed final-test count，构建立即 fail closed。

## 2. 构建入口

```powershell
./research/build_constant_baseline.ps1 -Version <new-immutable-version>
```

构建脚本会：

1. 用 Rust 校验 Series Result 和 Temporal Split 的 Dataset Manifest v1；
2. 核对 dataset 路径与 SHA-256；
3. 使用 `uv run --frozen` 执行 Python 入口；
4. 对相同输入构建两次并要求 artifact SHA-256 相同；
5. 输出 Git 忽略的 `artifacts/models/constant-baseline/<version>/`，并记录两份上游 manifest/hash、Python/NumPy/scikit-learn 版本和工作区代码状态。

Python 环境由 `pyproject.toml` 与 `uv.lock` 固定。MODEL-001 当前使用 Python 3.12、NumPy 2.5.2 和 scikit-learn 1.9.0。

## 3. 真实构建结果

固定版本：`2026-08-13.4d92b27.model001-v2`

| 项目 | 结果 |
|---|---:|
| Train series | 325 |
| Train team-1 wins | 170 |
| Constant `P(team_1_win)` | 0.5230769231 |
| Train Brier / Log Loss | 0.2494674556 / 0.6920817133 |
| Validation series | 349 |
| Validation Brier / Log Loss | 0.2479537478 / 0.6890521453 |
| Calibration series | 748 |
| Calibration Brier / Log Loss | 0.2474473942 / 0.6880387182 |
| Sealed final test | 356；未评估 |

Artifact SHA-256：`39e55ce8f3f5e17bf69ba9c44c6eba994336e1738cc608aeb4431d49b940b3b2`

相同输入双构建 hash 一致。artifact 中只保存 final-test count、membership commitment、access policy 和支持的指标，不保存 final-test IDs、labels 或指标。

## 4. 指标与边界

- Brier 使用 binary convention，即 `mean((y - p)^2)`，范围 `[0,1]`。
- Log Loss 使用自然对数，并显式固定 labels `[0,1]`，因此单一类别的 development slice 仍可计算。
- `team_1` 是数据合同中的稳定 outcome 方向，不表示主队、热门队或蓝色方。
- 当前 development 指标只是后续 Elo/统计模型的比较基线，不是模型有效性、可交易性或 M3 Gate 通过证据。
- final test 指标在技术上已具备同口径计算能力，但仍须等 model artifact/config/evaluation code 三个 SHA-256 冻结并显式 release；MODEL-001 不执行该 release。
