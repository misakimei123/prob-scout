# Leaguepedia 小样本记录

> Task：DATA-003
>
> 采集与验证日期：2026-08-12
>
> Attribution：Leaguepedia contributors / [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/)
>
> 用途：仅限 ProbScout 本地 Research；raw JSON 不提交、不再分发

## 1. 来源与访问方式

- 官方文档：[Leaguepedia API](https://lol.fandom.com/wiki/Help:Leaguepedia_API)
- 结构化入口：`https://lol.fandom.com/wiki/Special:CargoExport`
- Cargo tables：`MatchSchedule`、`TeamRedirects`、`Teams`、`TournamentRosters`
- 时间窗口：`2025-10-01 00:00:00 UTC` 至 `2025-11-15 00:00:00 UTC`，结束时间不包含
- 赛事过滤：`2025 Season World Championship%`
- 排序和条数：按 `MatchSchedule.DateTime_UTC DESC`，固定 10 场
- raw 与 manifest：`data/raw/leaguepedia/`，由 `.gitignore` 排除

`api.php?action=cargoquery` 在本次采集环境返回 `ratelimited`。脚本没有绕过限流，也没有抓取 HTML，而是使用 Cargo 扩展提供的官方 JSON export；每次显式刷新只发一个请求，默认运行只校验并复用本地 cache。

## 2. 可重复命令

首次下载或复用本地 cache：

```powershell
.\research\download_leaguepedia_sample.ps1
```

显式重新请求上游并比较响应 hash：

```powershell
.\research\download_leaguepedia_sample.ps1 -Refresh
```

脚本不带并发、翻页或自动 retry。请求失败或返回非 JSON 时直接 fail closed，并明确禁止回退到 HTML scraper。

## 3. Query 与响应证据

| Artifact | Value |
|---|---|
| Query SHA-256 | `c439223469688f5fb7524fd45f51fb3b502d0e1a7a36f2b5e9a34e0bbbe31115` |
| Raw response SHA-256 | `4a13de1023f409081867ea8c9b70208330923500abce4589599eadda0f608be7` |
| Raw response size | 8,095 bytes |
| Source captured at | `2026-08-12T06:15:19.3501349Z` |
| HTTP status / content type | `200` / `application/json` |

刷新后响应 hash 未变化；紧接着的第二次默认运行返回 `SourceStatus=cached`，且采集时间戳保持为原始响应时间，没有伪装成新的上游采集。

## 4. 字段与质量摘要

| 指标 | 结果 |
|---|---:|
| Rows | 10 |
| Unique `MatchId` | 10 |
| Unique canonical team pages | 11 |
| 双方规范队伍页标识完整 | 10 / 10 |
| 双方 roster 非空 | 10 / 10 |
| 单队 tournament roster 条目数 | 7–10 |
| 比赛时间范围 | `2025-10-25 05:00:00` 至 `2025-11-09 07:00:00` UTC |

每场包含以下字段组：

- 赛程与赛事：`MatchId`、`MatchStartUtc`、`BestOf`、`OverviewPage`、`DataPage`
- 双方标识：展示名 `Team1` / `Team2`、规范页面 ID `Team1Page` / `Team2Page`、简称 `Team1Short` / `Team2Short`
- 结果：`Team1Score`、`Team2Score`、`Winner`
- Roster：`Team1Roster`、`Team2Roster`，原始 Cargo list 使用 `;;` 分隔

一条不展开 roster 原文的标识示例：

| MatchId | Start UTC | Team 1 display / page / short | Team 2 display / page / short | BestOf | Event |
|---|---|---|---|---:|---|
| `2025 Season World Championship/Main Event_Finals_1` | `2025-11-09 07:00:00` | `T1` / `T1` / `T1` | `KT Rolster` / `KT Rolster` / `KT` | 5 | `2025 Season World Championship/Main Event` |

完整 10 场响应和 roster 只保存在 Git 忽略的本地 raw JSON；仓库只提交查询合同、hash、字段清单和聚合质量结果。

## 5. 结论与限制

- DATA-003 验收已满足：能够查询至少 10 场比赛、双方队伍标识，并保存独立的 UTC 来源采集时间。
- `Team1Page` / `Team2Page` 是本阶段优先使用的规范队伍标识；展示名和简称只作为匹配证据，不能单独承担长期实体 ID。
- `TournamentRosters` 是赛事报名阵容，不等同于某局实际首发；后续特征工程必须保持这一语义边界。
- 样本只覆盖一个赛事窗口，不能证明 Leaguepedia 全历史 schema、别名和 roster 都完整。
- Leaguepedia 是社区维护数据，后续构建数据集时仍需处理历史修订、字段漂移和来源时点泄漏。
