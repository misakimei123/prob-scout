# Oracle's Elixir 小样本记录

> Task：DATA-002
>
> 采集与验证日期：2026-08-12
>
> 用途：仅限 ProbScout 本地 Research；raw CSV 不提交、不再分发

## 1. 来源与范围

- 官方入口：[Oracle's Elixir Match Data Downloads](https://oracleselixir.com/tools/downloads)
- 官方目录：[Google Drive 年度 CSV](https://drive.google.com/drive/folders/1gLSw0RLjBbtaNy0dgnGQDAZOHIgCe-HH)
- 官方文件：`2025_LoL_esports_match_data_from_OraclesElixir.csv`
- Google Drive file ID：`1v6LRphp2kYciU4SXp0PCjEMuev1bDejc`
- 小样本窗口：`2025-01-15` 至 `2025-01-21`，不限定 league
- raw 与 manifest：`data/raw/oracles_elixir/`，由 `.gitignore` 排除

项目不发布 raw CSV。本文只提交可复现命令、hash 和不含原始记录的聚合摘要。

## 2. 可重复命令

默认命令会优先校验并复用本地 source cache；本地没有 cache 时通过固定版本 `gdown 6.1.0` 下载官方文件：

```powershell
.\research\download_oracles_elixir_sample.ps1
```

显式重新核对官方文件：

```powershell
.\research\download_oracles_elixir_sample.ps1 -RefreshSource
```

Google Drive 可能临时触发公开下载配额。此时只允许从上面的官方文件页面下载同名 CSV，再导入本地文件；不切换第三方镜像：

```powershell
.\research\download_oracles_elixir_sample.ps1 `
  -LocalSourcePath "<官方下载文件的绝对路径>"
```

脚本会验证文件名、`gameid`/`date` 表头和 SHA-256，再复制到 Git 忽略目录。重复运行不会覆盖相同 hash 文件。

## 3. Hash 与体积

| Artifact | Size | SHA-256 |
|---|---:|---|
| 2025 官方年度 source | 79,169,638 bytes | `c9a158b9e0a965a47d31d3674c127a26f75e6c91a324bd1858e4784b1336214a` |
| 2025-01-15 至 2025-01-21 sample | 1,642,539 bytes | `107c64b631df79208a53f34c6582349e402f5234d62e972d4644fdeba159f923` |

以上 hash 是 2026-08-12 的实际下载结果。Oracle's Elixir 可能修订历史记录；未来 `-RefreshSource` 若得到新 hash，必须先检查变更，再更新本文，不能静默替换研究基线。

## 4. 字段与质量摘要

| 指标 | 结果 |
|---|---:|
| Source rows | 120,492 |
| Sample rows | 1,680 |
| Unique games | 140 |
| Columns | 165 |
| 每个 game 的 rows | 12 |
| Invalid date rows | 0 |
| `result=0` / `result=1` | 840 / 840 |
| `datacompleteness=complete` | 1,392 |
| `datacompleteness=partial` | 288 |

样本覆盖 `HLL`、`LCK`、`LCKC`、`LCP`、`LEC`、`LFL2`、`LPL`、`LVP SL`、`NLC`、`PRM`。`top`、`jng`、`mid`、`bot`、`sup`、`team` 各 280 行，符合每局双方各 5 名选手加 1 条 team row 的 12-row 结构。

本阶段重点字段如下：

- 标识与时间：`gameid`、`date`、`league`、`year`、`split`、`playoffs`、`game`、`patch`
- 参赛方：`participantid`、`side`、`position`、`playername`、`playerid`、`teamname`、`teamid`
- 结果与完整度：`result`、`datacompleteness`
- 局级统计：`gamelength`、kills/deaths/assists、objectives、gold、CS、vision 和 10/15/20/25 分钟快照字段

关键字段 `gameid`、`date`、`league`、`position`、`teamname`、`result`、`datacompleteness` 在该样本中均为 0 个空值。完整 165 列清单和 null count 保存在本地 manifest 中，不提交第三方 raw 内容。

## 5. 已验证行为与限制

- 首次官方文件导入返回 `SourceStatus=imported`、`SampleStatus=created`。
- 第二次无网络运行返回 `SourceStatus=cached`、`SampleStatus=unchanged`，source/sample hash 均未变化。
- `data/raw/oracles_elixir/**` 已由 `/data/` 规则排除，`git status` 不显示 CSV 或 manifest。
- 288 条 `partial` row 对应 24 个 game。后续模型任务不得直接把 partial 数据与 complete 数据混合，必须先定义过滤或缺失值策略。
- 该样本只证明 Oracle's Elixir 文件可获取、可解析和可重复缓存，不证明数据适合真钱交易，也不证明历史策略存在 Alpha。
