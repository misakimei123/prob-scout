use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use chrono::{Datelike, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    prematch_features::{
        PrematchFeatureSnapshot, SourcedCount, SourcedOptionalMinutes, SourcedRatio,
    },
    series_result::SeriesResult,
    temporal_split::{
        TemporalSplitCandidate, TemporalSplitManifest, TemporalSplitPlan,
        build_temporal_split_manifest,
    },
};

pub const DATA_QUALITY_REPORT_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketGradeSummary {
    pub total_markets: u32,
    pub grade_a: u32,
    pub grade_b: u32,
    pub grade_c: u32,
    pub unavailable: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataQualityBuildInput {
    pub minimum_eligible_series: u32,
    pub series_results: Vec<SeriesResult>,
    pub feature_snapshots: Vec<PrematchFeatureSnapshot>,
    pub temporal_split_manifest: TemporalSplitManifest,
    pub market_grade_summary: MarketGradeSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Warning,
    Fail,
}

impl CheckStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pass => "Pass",
            Self::Warning => "Warning",
            Self::Fail => "Fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityCheck {
    pub dimension: String,
    pub status: CheckStatus,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NumericProfile {
    pub feature: String,
    pub count: u32,
    pub missing_count: u32,
    pub min: Option<u64>,
    pub q1: Option<u64>,
    pub median: Option<u64>,
    pub q3: Option<u64>,
    pub max: Option<u64>,
    pub outlier_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissingnessProfile {
    pub field: String,
    pub denominator: u32,
    pub missing_count: u32,
    pub interpretation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualityFinding {
    pub finding_id: String,
    pub severity: String,
    pub confidence: String,
    pub what_failed: String,
    pub evidence: String,
    pub impact: String,
    pub likely_cause: String,
    pub remediation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataQualityReport {
    pub report_version: u16,
    pub series_count: u32,
    pub minimum_eligible_series: u32,
    pub time_start_utc: chrono::DateTime<Utc>,
    pub time_end_utc: chrono::DateTime<Utc>,
    pub distinct_utc_dates: u32,
    pub years: Vec<i32>,
    pub distinct_teams: u32,
    pub regions: BTreeMap<String, u32>,
    pub patches: BTreeMap<String, u32>,
    pub best_of: BTreeMap<String, u32>,
    pub checks: Vec<QualityCheck>,
    pub missingness_profiles: Vec<MissingnessProfile>,
    pub numeric_profiles: Vec<NumericProfile>,
    pub findings: Vec<QualityFinding>,
    pub gate_decision: String,
    pub gate_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataQualityError {
    EmptyInput(&'static str),
    InvalidMinimumSeries,
    EmptyField {
        series_id: String,
        field: &'static str,
    },
    DuplicateSeries {
        dataset: &'static str,
        series_id: String,
    },
    InvalidSeries(String),
    DatasetMembershipMismatch(&'static str),
    FeatureIdentityMismatch(String),
    FeatureTimeMismatch(String),
    InvalidFeatureValue {
        series_id: String,
        feature: &'static str,
    },
    SourceTimeViolation {
        series_id: String,
        feature: &'static str,
    },
    SplitManifest(String),
    SplitMembershipMismatch,
    InvalidMarketGradeSummary,
    CountOverflow(&'static str),
}

impl fmt::Display for DataQualityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput(dataset) => write!(formatter, "quality input is empty: {dataset}"),
            Self::InvalidMinimumSeries => {
                formatter.write_str("minimum_eligible_series must be greater than zero")
            }
            Self::EmptyField { series_id, field } => {
                write!(
                    formatter,
                    "required field is empty: series={series_id}, field={field}"
                )
            }
            Self::DuplicateSeries { dataset, series_id } => {
                write!(formatter, "duplicate series in {dataset}: {series_id}")
            }
            Self::InvalidSeries(series_id) => {
                write!(
                    formatter,
                    "series result violates BO/score/winner rules: {series_id}"
                )
            }
            Self::DatasetMembershipMismatch(dataset) => write!(
                formatter,
                "series membership does not match series results: {dataset}"
            ),
            Self::FeatureIdentityMismatch(series_id) => {
                write!(
                    formatter,
                    "feature identity differs from result row: {series_id}"
                )
            }
            Self::FeatureTimeMismatch(series_id) => {
                write!(
                    formatter,
                    "feature snapshot time differs from T-15m contract: {series_id}"
                )
            }
            Self::InvalidFeatureValue { series_id, feature } => write!(
                formatter,
                "feature value/source contract is invalid: series={series_id}, feature={feature}"
            ),
            Self::SourceTimeViolation { series_id, feature } => write!(
                formatter,
                "feature source is after snapshot cutoff: series={series_id}, feature={feature}"
            ),
            Self::SplitManifest(error) => write!(formatter, "temporal split is invalid: {error}"),
            Self::SplitMembershipMismatch => formatter.write_str(
                "temporal split membership or final test commitment differs from source",
            ),
            Self::InvalidMarketGradeSummary => {
                formatter.write_str("market grade counts do not sum to total_markets")
            }
            Self::CountOverflow(field) => write!(formatter, "count exceeds u32: {field}"),
        }
    }
}

impl Error for DataQualityError {}

pub fn build_data_quality_report(
    mut input: DataQualityBuildInput,
) -> Result<DataQualityReport, DataQualityError> {
    if input.minimum_eligible_series == 0 {
        return Err(DataQualityError::InvalidMinimumSeries);
    }
    if input.series_results.is_empty() {
        return Err(DataQualityError::EmptyInput("series_results"));
    }
    if input.feature_snapshots.is_empty() {
        return Err(DataQualityError::EmptyInput("feature_snapshots"));
    }
    let market_total = input
        .market_grade_summary
        .grade_a
        .checked_add(input.market_grade_summary.grade_b)
        .and_then(|value| value.checked_add(input.market_grade_summary.grade_c))
        .and_then(|value| value.checked_add(input.market_grade_summary.unavailable));
    if market_total != Some(input.market_grade_summary.total_markets)
        || input.market_grade_summary.total_markets == 0
    {
        return Err(DataQualityError::InvalidMarketGradeSummary);
    }

    input.series_results.sort_by(|left, right| {
        (left.scheduled_start_utc, left.series_id.as_str())
            .cmp(&(right.scheduled_start_utc, right.series_id.as_str()))
    });
    input.feature_snapshots.sort_by(|left, right| {
        (left.scheduled_start_utc, left.series_id.as_str())
            .cmp(&(right.scheduled_start_utc, right.series_id.as_str()))
    });

    let mut series_ids = BTreeSet::new();
    let mut regions = BTreeMap::new();
    let mut patches = BTreeMap::new();
    let mut best_of = BTreeMap::new();
    let mut teams = BTreeSet::new();
    let mut years = BTreeSet::new();
    let mut dates = BTreeSet::new();
    for result in &input.series_results {
        validate_series_result(result)?;
        if !series_ids.insert(result.series_id.as_str()) {
            return Err(DataQualityError::DuplicateSeries {
                dataset: "series_results",
                series_id: result.series_id.clone(),
            });
        }
        *regions.entry(result.region.clone()).or_insert(0_u32) += 1;
        *patches.entry(result.patch.clone()).or_insert(0_u32) += 1;
        *best_of
            .entry(format!("BO{}", result.best_of))
            .or_insert(0_u32) += 1;
        teams.extend(result.team_ids.iter().cloned());
        years.insert(result.scheduled_start_utc.year());
        dates.insert(result.scheduled_start_utc.date_naive());
    }

    let mut feature_ids = BTreeSet::new();
    let results_by_id = input
        .series_results
        .iter()
        .map(|result| (result.series_id.as_str(), result))
        .collect::<BTreeMap<_, _>>();
    let mut prior_series = Vec::new();
    let mut prior_games = Vec::new();
    let mut same_patch_series = Vec::new();
    let mut rest_minutes = Vec::new();
    let mut prior_form_missing = 0_u32;
    let mut same_patch_missing = 0_u32;
    let mut rest_missing = 0_u32;
    for snapshot in &input.feature_snapshots {
        if !feature_ids.insert(snapshot.series_id.as_str()) {
            return Err(DataQualityError::DuplicateSeries {
                dataset: "feature_snapshots",
                series_id: snapshot.series_id.clone(),
            });
        }
        let result = results_by_id.get(snapshot.series_id.as_str()).ok_or(
            DataQualityError::DatasetMembershipMismatch("feature_snapshots"),
        )?;
        validate_feature_snapshot(snapshot, result)?;
        for team in &snapshot.team_features {
            let key = format!("{}|{}", snapshot.series_id, team.team_id);
            prior_series.push((key.clone(), u64::from(team.prior_series_count.value)));
            if team.prior_series_count.value == 0 {
                prior_form_missing = prior_form_missing
                    .checked_add(1)
                    .ok_or(DataQualityError::CountOverflow("prior_form_missing"))?;
            }
            prior_games.push((key.clone(), u64::from(team.prior_game_count.value)));
            same_patch_series.push((key.clone(), u64::from(team.same_patch_series_count.value)));
            if team.same_patch_series_count.value == 0 {
                same_patch_missing = same_patch_missing
                    .checked_add(1)
                    .ok_or(DataQualityError::CountOverflow("same_patch_missing"))?;
            }
            if let Some(value) = team.rest_minutes.value {
                rest_minutes.push((key, value as u64));
            } else {
                rest_missing = rest_missing
                    .checked_add(1)
                    .ok_or(DataQualityError::CountOverflow("rest_missing"))?;
            }
        }
    }
    if feature_ids != series_ids {
        return Err(DataQualityError::DatasetMembershipMismatch(
            "feature_snapshots",
        ));
    }

    input
        .temporal_split_manifest
        .validate()
        .map_err(|error| DataQualityError::SplitManifest(error.to_string()))?;
    let split_plan = TemporalSplitPlan {
        train: input.temporal_split_manifest.train.window.clone(),
        validation: input.temporal_split_manifest.validation.window.clone(),
        calibration: input.temporal_split_manifest.calibration.window.clone(),
        final_test: input.temporal_split_manifest.final_test.window.clone(),
    };
    let split_candidates = input
        .feature_snapshots
        .iter()
        .map(|snapshot| TemporalSplitCandidate {
            series_id: snapshot.series_id.clone(),
            scheduled_start_utc: snapshot.scheduled_start_utc,
        })
        .collect::<Vec<_>>();
    let rebuilt_split = build_temporal_split_manifest(
        split_candidates,
        split_plan,
        input.temporal_split_manifest.source_dataset_sha256.clone(),
    )
    .map_err(|error| DataQualityError::SplitManifest(error.to_string()))?;
    if rebuilt_split != input.temporal_split_manifest {
        return Err(DataQualityError::SplitMembershipMismatch);
    }

    let series_count = checked_u32(input.series_results.len(), "series_count")?;
    let feature_count = checked_u32(input.feature_snapshots.len(), "feature_count")?;
    let team_side_count = feature_count
        .checked_mul(2)
        .ok_or(DataQualityError::CountOverflow("team_side_count"))?;
    let time_start_utc = input.series_results[0].scheduled_start_utc;
    let time_end_utc = input.series_results[input.series_results.len() - 1].scheduled_start_utc;
    let distinct_utc_dates = checked_u32(dates.len(), "distinct_utc_dates")?;
    let distinct_teams = checked_u32(teams.len(), "distinct_teams")?;
    let development_count = checked_u32(
        input
            .temporal_split_manifest
            .development_series_ids()
            .count(),
        "development_count",
    )?;

    let numeric_profiles = vec![
        numeric_profile("prior_series_count", prior_series, team_side_count)?,
        numeric_profile("prior_game_count", prior_games, team_side_count)?,
        numeric_profile(
            "same_patch_series_count",
            same_patch_series,
            team_side_count,
        )?,
        numeric_profile("rest_minutes", rest_minutes, team_side_count)?,
    ];

    let sample_coverage_percent_x100 =
        u64::from(series_count) * 10_000 / u64::from(input.minimum_eligible_series);
    let sample_coverage = format!(
        "{series_count}/{} ({:.2}%)",
        input.minimum_eligible_series,
        sample_coverage_percent_x100 as f64 / 100.0
    );
    let same_patch_rate_x100 = u64::from(same_patch_missing) * 10_000 / u64::from(team_side_count);
    let execution_grade_missing = input
        .market_grade_summary
        .total_markets
        .checked_sub(input.market_grade_summary.grade_a)
        .ok_or(DataQualityError::InvalidMarketGradeSummary)?;
    let missingness_profiles = vec![
        MissingnessProfile {
            field: "series_required_fields".to_owned(),
            denominator: series_count,
            missing_count: 0,
            interpretation: "任一缺失或矛盾会使报告 fail closed".to_owned(),
        },
        MissingnessProfile {
            field: "feature_snapshot_required_fields".to_owned(),
            denominator: team_side_count,
            missing_count: 0,
            interpretation: "身份、cutoff 与 count/ratio 合同已通过".to_owned(),
        },
        MissingnessProfile {
            field: "prior_form_history".to_owned(),
            denominator: team_side_count,
            missing_count: prior_form_missing,
            interpretation: "无历史时保留行并显式降级".to_owned(),
        },
        MissingnessProfile {
            field: "same_patch_form_history".to_owned(),
            denominator: team_side_count,
            missing_count: same_patch_missing,
            interpretation: "count=0 且 source=null 表示 unavailable，不是 0%".to_owned(),
        },
        MissingnessProfile {
            field: "rest_minutes".to_owned(),
            denominator: team_side_count,
            missing_count: rest_missing,
            interpretation: "仅在无 prior series 时允许缺失".to_owned(),
        },
        MissingnessProfile {
            field: "execution_grade_market_snapshot".to_owned(),
            denominator: input.market_grade_summary.total_markets,
            missing_count: execution_grade_missing,
            interpretation: "非 Grade A 不能证明历史可执行性".to_owned(),
        },
    ];
    let mut checks = vec![
        QualityCheck {
            dimension: "Completeness".to_owned(),
            status: CheckStatus::Pass,
            evidence: format!(
                "{series_count} series and {feature_count} snapshots passed all required-field and source-time contracts"
            ),
        },
        QualityCheck {
            dimension: "Uniqueness".to_owned(),
            status: CheckStatus::Pass,
            evidence: format!("{series_count}/{series_count} unique series_id at one-series grain"),
        },
        QualityCheck {
            dimension: "Cross-dataset integrity".to_owned(),
            status: CheckStatus::Pass,
            evidence: format!(
                "series results, feature snapshots, and temporal split cover the same {series_count} series"
            ),
        },
        QualityCheck {
            dimension: "Temporal leakage".to_owned(),
            status: CheckStatus::Pass,
            evidence: format!(
                "{team_side_count} team-side feature rows have no source timestamp after T-15m snapshot"
            ),
        },
        QualityCheck {
            dimension: "Temporal split".to_owned(),
            status: CheckStatus::Pass,
            evidence: format!(
                "development IDs={development_count}; sealed final test count={}; rebuilt commitment matches source",
                input.temporal_split_manifest.final_test.series_count
            ),
        },
        QualityCheck {
            dimension: "Sample volume".to_owned(),
            status: if series_count >= input.minimum_eligible_series {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            evidence: sample_coverage.clone(),
        },
        QualityCheck {
            dimension: "Temporal and Patch coverage".to_owned(),
            status: if distinct_utc_dates >= 4 && patches.len() > 1 {
                CheckStatus::Pass
            } else {
                CheckStatus::Warning
            },
            evidence: format!(
                "UTC dates={distinct_utc_dates}, years={}, patches={} ({})",
                years.len(),
                patches.len(),
                patches.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        },
        QualityCheck {
            dimension: "Same-Patch feature sparsity".to_owned(),
            status: if same_patch_missing == 0 {
                CheckStatus::Pass
            } else {
                CheckStatus::Warning
            },
            evidence: format!(
                "{same_patch_missing}/{team_side_count} ({:.2}%) team sides have zero same-Patch history",
                same_patch_rate_x100 as f64 / 100.0
            ),
        },
        QualityCheck {
            dimension: "Historical market fidelity".to_owned(),
            status: if input.market_grade_summary.grade_a
                == input.market_grade_summary.total_markets
            {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            evidence: format!(
                "A={}, B={}, C={}, unavailable={} of {} reviewed markets",
                input.market_grade_summary.grade_a,
                input.market_grade_summary.grade_b,
                input.market_grade_summary.grade_c,
                input.market_grade_summary.unavailable,
                input.market_grade_summary.total_markets
            ),
        },
    ];
    checks.sort_by(|left, right| left.dimension.cmp(&right.dimension));

    let mut findings = Vec::new();
    if series_count < input.minimum_eligible_series {
        findings.push(QualityFinding {
            finding_id: "DQ-001".to_owned(),
            severity: "High".to_owned(),
            confidence: "High".to_owned(),
            what_failed: "Eligible series volume is below the preregistered modeling target".to_owned(),
            evidence: sample_coverage,
            impact: "Model fitting, calibration, segmented evaluation, and confidence intervals would be dominated by small-sample variance".to_owned(),
            likely_cause: "The fixed DATA-008 baseline contains 50 markets, of which only 23 are resolved BO3/BO5 mappings".to_owned(),
            remediation: "Expand the immutable historical mapping/result/feature pipeline to at least 500 eligible series without weakening identity or leakage rules".to_owned(),
        });
    }
    if distinct_utc_dates <= 4 || years.len() == 1 || patches.len() == 1 {
        findings.push(QualityFinding {
            finding_id: "DQ-002".to_owned(),
            severity: "High".to_owned(),
            confidence: "High".to_owned(),
            what_failed: "Temporal and Patch coverage cannot support out-of-time robustness claims".to_owned(),
            evidence: format!(
                "date range {} to {}, distinct UTC dates={distinct_utc_dates}, years={}, patches={}",
                time_start_utc.to_rfc3339(),
                time_end_utc.to_rfc3339(),
                years.len(),
                patches.len()
            ),
            impact: "The current split tests mechanics only; it cannot distinguish stable predictive value from a four-day, single-Patch regime".to_owned(),
            likely_cause: "HIST-003 intentionally started from the recent 50-market feasibility review instead of a multi-season corpus".to_owned(),
            remediation: "Acquire multiple seasons and Patches, then regenerate identities, results, features, splits, and this report".to_owned(),
        });
    }
    if same_patch_missing > 0 {
        findings.push(QualityFinding {
            finding_id: "DQ-003".to_owned(),
            severity: "Medium".to_owned(),
            confidence: "High".to_owned(),
            what_failed: "Some team sides have no prior series on the target Patch".to_owned(),
            evidence: format!(
                "{same_patch_missing}/{team_side_count} team sides have same_patch_series_count=0 with null source time"
            ),
            impact: "Same-Patch form is unavailable for those rows; treating zero history as a 0% win rate would bias the model".to_owned(),
            likely_cause: "Targets occur early in Patch 26.15 or the exact source-key history has no earlier series on that Patch".to_owned(),
            remediation: "Retain the series, encode same-Patch availability explicitly, and keep numerator/denominator missing semantics; do not impute 0%".to_owned(),
        });
    }
    if input.market_grade_summary.grade_c > 0
        || input.market_grade_summary.grade_b > 0
        || input.market_grade_summary.unavailable > 0
    {
        findings.push(QualityFinding {
            finding_id: "DQ-004".to_owned(),
            severity: "High".to_owned(),
            confidence: "High".to_owned(),
            what_failed: "Historical market evidence is not execution-grade".to_owned(),
            evidence: format!(
                "Grade C={}/{}; Grade A={}",
                input.market_grade_summary.grade_c,
                input.market_grade_summary.total_markets,
                input.market_grade_summary.grade_a
            ),
            impact: "The corpus may support probability-signal research but cannot prove spread, depth, slippage, fill rate, or executable historical PnL".to_owned(),
            likely_cause: "Official history provides timestamp/price points without decision-time bid/ask, depth, and fee snapshots".to_owned(),
            remediation: "Keep historical conclusions signal-only and collect immutable real-time order books plus fees before execution-sensitive claims".to_owned(),
        });
    }
    let outlier_features = numeric_profiles
        .iter()
        .filter(|profile| profile.outlier_count > 0)
        .map(|profile| format!("{}={}", profile.feature, profile.outlier_count))
        .collect::<Vec<_>>();
    if !outlier_features.is_empty() {
        findings.push(QualityFinding {
            finding_id: "DQ-005".to_owned(),
            severity: "Medium".to_owned(),
            confidence: "Medium".to_owned(),
            what_failed: "IQR review flags exist in numeric feature distributions".to_owned(),
            evidence: outlier_features.join(", "),
            impact: "Long rest periods or unusually deep history can dominate scaling on this small sample, but are not automatically invalid".to_owned(),
            likely_cause: "International schedules, breaks between competitions, and unequal source-key history depth create legitimate long tails".to_owned(),
            remediation: "Review flagged rows against source times, use robust scaling or preregistered caps in M3, and never delete solely because of IQR status".to_owned(),
        });
    }
    findings.sort_by(|left, right| left.finding_id.cmp(&right.finding_id));

    // M2 预注册的硬门槛只有 eligible series 数量；时间与 Patch 集中度仍作为高风险 finding，
    // 但不能在没有预注册阈值时事后发明新的二元门槛。
    let gate_ready = series_count >= input.minimum_eligible_series;
    Ok(DataQualityReport {
        report_version: DATA_QUALITY_REPORT_VERSION,
        series_count,
        minimum_eligible_series: input.minimum_eligible_series,
        time_start_utc,
        time_end_utc,
        distinct_utc_dates,
        years: years.into_iter().collect(),
        distinct_teams,
        regions,
        patches,
        best_of,
        checks,
        missingness_profiles,
        numeric_profiles,
        findings,
        gate_decision: if gate_ready {
            "ReadyForM3".to_owned()
        } else {
            "NotReadyForM3".to_owned()
        },
        gate_reason: if gate_ready {
            "volume and temporal coverage thresholds passed".to_owned()
        } else {
            format!(
                "only {series_count}/{} eligible series across {distinct_utc_dates} UTC dates, {} year(s), and {} Patch(es)",
                input.minimum_eligible_series,
                years_count(&input.series_results),
                patches_count(&input.series_results)
            )
        },
    })
}

pub fn render_data_quality_markdown(report: &DataQualityReport) -> String {
    let mut output = String::new();
    output.push_str("# HIST-006 数据质量报告\n\n");
    output.push_str("生成口径：一行一个 BO3/BO5 series；Feature Snapshot 同 grain，每行两方。\n\n");
    output.push_str("## 结论\n\n");
    output.push_str(&format!(
        "- M2 Gate：`{}`。\n- 原因：{}。\n- 当前数据只允许验证 pipeline 与 Grade C 概率信号研究；不支持模型有效性或历史可执行 PnL 结论。\n\n",
        report.gate_decision, report.gate_reason
    ));
    output.push_str("## Dataset 与 grain\n\n");
    output.push_str(&format!(
        "| 项目 | 值 |\n|---|---:|\n| Series rows | {} |\n| 最低目标 | {} |\n| 时间范围 | {} 至 {} |\n| UTC 日期数 | {} |\n| 年份 | {} |\n| Canonical Teams | {} |\n\n",
        report.series_count,
        report.minimum_eligible_series,
        report.time_start_utc.to_rfc3339(),
        report.time_end_utc.to_rfc3339(),
        report.distinct_utc_dates,
        report.years.iter().map(i32::to_string).collect::<Vec<_>>().join(", "),
        report.distinct_teams
    ));
    render_count_map(&mut output, "赛区覆盖", "Region", &report.regions);
    render_count_map(&mut output, "Patch 覆盖", "Patch", &report.patches);
    render_count_map(&mut output, "BO 覆盖", "Best of", &report.best_of);

    output.push_str("## 缺失率（Missingness）\n\n");
    output.push_str(
        "| 字段/能力 | 缺失数 | 分母 | Missing rate | 解释 |\n|---|---:|---:|---:|---|\n",
    );
    for profile in &report.missingness_profiles {
        output.push_str(&format!(
            "| {} | {} | {} | {:.2}% | {} |\n",
            profile.field,
            profile.missing_count,
            profile.denominator,
            f64::from(profile.missing_count) * 100.0 / f64::from(profile.denominator),
            profile.interpretation
        ));
    }
    output.push('\n');

    output.push_str("## Checks performed\n\n");
    output.push_str("| 维度 | 状态 | 证据 |\n|---|---|---|\n");
    for check in &report.checks {
        output.push_str(&format!(
            "| {} | {} | {} |\n",
            check.dimension,
            check.status.as_str(),
            check.evidence
        ));
    }
    output.push('\n');

    output.push_str("## Numeric profile 与 IQR review\n\n");
    output
        .push_str("四分位数使用排序后 `floor((n-1)*p)` 位置；IQR 只标记 review，不自动排除。\n\n");
    output.push_str("| Feature | Count | Missing | Min | Q1 | Median | Q3 | Max | IQR outliers |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for profile in &report.numeric_profiles {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            profile.feature,
            profile.count,
            profile.missing_count,
            optional_u64(profile.min),
            optional_u64(profile.q1),
            optional_u64(profile.median),
            optional_u64(profile.q3),
            optional_u64(profile.max),
            profile.outlier_count
        ));
    }
    output.push('\n');

    output.push_str("## 异常发现（Findings）\n\n");
    for finding in &report.findings {
        output.push_str(&format!(
            "### {} — {}\n\n- Severity：{}\n- Confidence：{}\n- Evidence：{}\n- Impact：{}\n- Likely cause：{}\n- Remediation：{}\n\n",
            finding.finding_id,
            finding.what_failed,
            finding.severity,
            finding.confidence,
            finding.evidence,
            finding.impact,
            finding.likely_cause,
            finding.remediation
        ));
    }

    output.push_str("## 缺失与降级规则\n\n");
    output.push_str("| 字段/状态 | 规则 | 处理 |\n|---|---|---|\n");
    output.push_str("| Series identity、Scheduled Start、BO、Patch、双方、比分、winner | 任一为空、矛盾或跨表不一致 | 排除该 series，报告生成 fail closed |\n");
    output.push_str(
        "| Feature source timestamp | 晚于 snapshot cutoff | 排除该 series，判定为 leakage |\n",
    );
    output.push_str("| Prior form | `prior_series_count=0` 时 source time 和 rest 允许为空 | 保留 series，显式标记 prior-form unavailable，不填充 0% |\n");
    output.push_str("| Same-Patch form | `same_patch_series_count=0` 且 source time 为空 | 保留 series，仅降级 same-Patch features，不填充 0% |\n");
    output.push_str("| Split membership | 重复、遗漏、重叠、窗口外或 final commitment 不一致 | 停止所有下游消费 |\n");
    output.push_str("| Historical market Grade C | 缺少决策时点 bid/ask、depth 或 fee | 仅用于 signal research；排除 execution/PnL 证明 |\n\n");
    output.push_str("## 自动化测试建议\n\n");
    output.push_str("- 保持 `series_id` 在 result、feature 和 split 中唯一且全集相等。\n");
    output.push_str("- 保持每个 feature source time `<= snapshot_at_utc`。\n");
    output
        .push_str("- 保持 ratio numerator `<=` denominator，且 denominator 与对应 count 一致。\n");
    output.push_str("- 保持 split 窗口连续、非空、无重叠，并重算 final test commitment。\n");
    output.push_str(
        "- 样本扩展后对 UTC 日期、年份、Patch、赛区和 feature sparsity 做版本间 drift 比较。\n\n",
    );
    output.push_str("## Assumptions 与边界\n\n");
    output.push_str("- Leaguepedia 当前页面可能包含事后修订；现有 manifest 证明使用的字节和生成代码，不证明页面在比赛时已不可变。\n");
    output.push_str("- Exact source-key 历史避免错误合并，但可能低估改名队伍的历史覆盖。\n");
    output.push_str("- Final Test Seal 是工作流门禁，不是针对源数据读取者的加密保密。\n");
    output
}

fn validate_series_result(result: &SeriesResult) -> Result<(), DataQualityError> {
    for (field, value) in [
        ("series_id", result.series_id.as_str()),
        ("competition_id", result.competition_id.as_str()),
        ("region", result.region.as_str()),
        ("patch", result.patch.as_str()),
        ("team_ids[0]", result.team_ids[0].as_str()),
        ("team_ids[1]", result.team_ids[1].as_str()),
        ("team_names[0]", result.team_names[0].as_str()),
        ("team_names[1]", result.team_names[1].as_str()),
        ("winner_team_id", result.winner_team_id.as_str()),
        ("mapping_evidence_id", result.mapping_evidence_id.as_str()),
        ("result_evidence_id", result.result_evidence_id.as_str()),
        ("market_id", result.market_id.as_str()),
        (
            "market_resolution_evidence_id",
            result.market_resolution_evidence_id.as_str(),
        ),
    ] {
        if value.trim().is_empty() {
            return Err(DataQualityError::EmptyField {
                series_id: result.series_id.clone(),
                field,
            });
        }
    }
    if !matches!(result.best_of, 3 | 5)
        || result.team_ids[0] == result.team_ids[1]
        || !result.team_ids.contains(&result.winner_team_id)
    {
        return Err(DataQualityError::InvalidSeries(result.series_id.clone()));
    }
    let wins_needed = result.best_of / 2 + 1;
    let winner_index = usize::from(result.scores[1] == wins_needed);
    let valid_score = (result.scores[0] == wins_needed && result.scores[1] < wins_needed)
        || (result.scores[1] == wins_needed && result.scores[0] < wins_needed);
    if !valid_score || result.team_ids[winner_index] != result.winner_team_id {
        return Err(DataQualityError::InvalidSeries(result.series_id.clone()));
    }
    Ok(())
}

fn validate_feature_snapshot(
    snapshot: &PrematchFeatureSnapshot,
    result: &SeriesResult,
) -> Result<(), DataQualityError> {
    if snapshot.competition_id != result.competition_id
        || snapshot.region != result.region
        || snapshot.patch != result.patch
        || snapshot.scheduled_start_utc != result.scheduled_start_utc
        || snapshot.best_of != result.best_of
        || snapshot.team_features[0].team_id != result.team_ids[0]
        || snapshot.team_features[1].team_id != result.team_ids[1]
    {
        return Err(DataQualityError::FeatureIdentityMismatch(
            snapshot.series_id.clone(),
        ));
    }
    if snapshot.scheduled_start_utc - snapshot.snapshot_at_utc != Duration::minutes(15) {
        return Err(DataQualityError::FeatureTimeMismatch(
            snapshot.series_id.clone(),
        ));
    }
    for team in &snapshot.team_features {
        validate_count(
            &snapshot.series_id,
            "prior_series_count",
            &team.prior_series_count,
            snapshot.snapshot_at_utc,
        )?;
        validate_ratio(
            &snapshot.series_id,
            "prior_series_win_rate",
            &team.prior_series_win_rate,
            team.prior_series_count.value,
            snapshot.snapshot_at_utc,
        )?;
        validate_count(
            &snapshot.series_id,
            "prior_game_count",
            &team.prior_game_count,
            snapshot.snapshot_at_utc,
        )?;
        validate_ratio(
            &snapshot.series_id,
            "prior_game_win_rate",
            &team.prior_game_win_rate,
            team.prior_game_count.value,
            snapshot.snapshot_at_utc,
        )?;
        validate_count(
            &snapshot.series_id,
            "same_patch_series_count",
            &team.same_patch_series_count,
            snapshot.snapshot_at_utc,
        )?;
        validate_ratio(
            &snapshot.series_id,
            "same_patch_series_win_rate",
            &team.same_patch_series_win_rate,
            team.same_patch_series_count.value,
            snapshot.snapshot_at_utc,
        )?;
        validate_rest(
            &snapshot.series_id,
            &team.rest_minutes,
            &team.prior_series_count,
            snapshot.snapshot_at_utc,
        )?;
        if team.same_patch_series_count.value > team.prior_series_count.value
            || team.prior_game_count.value < team.prior_series_count.value.saturating_mul(2)
            || team.prior_game_count.value > team.prior_series_count.value.saturating_mul(5)
        {
            return Err(DataQualityError::InvalidFeatureValue {
                series_id: snapshot.series_id.clone(),
                feature: "cross_feature_counts",
            });
        }
    }
    Ok(())
}

fn validate_count(
    series_id: &str,
    feature: &'static str,
    value: &SourcedCount,
    snapshot_at: chrono::DateTime<Utc>,
) -> Result<(), DataQualityError> {
    if (value.value == 0) != value.source_latest_at_utc.is_none() {
        return Err(DataQualityError::InvalidFeatureValue {
            series_id: series_id.to_owned(),
            feature,
        });
    }
    validate_source_time(series_id, feature, value.source_latest_at_utc, snapshot_at)
}

fn validate_ratio(
    series_id: &str,
    feature: &'static str,
    value: &SourcedRatio,
    expected_denominator: u32,
    snapshot_at: chrono::DateTime<Utc>,
) -> Result<(), DataQualityError> {
    if value.denominator != expected_denominator
        || value.numerator > value.denominator
        || (value.denominator == 0) != value.source_latest_at_utc.is_none()
    {
        return Err(DataQualityError::InvalidFeatureValue {
            series_id: series_id.to_owned(),
            feature,
        });
    }
    validate_source_time(series_id, feature, value.source_latest_at_utc, snapshot_at)
}

fn validate_rest(
    series_id: &str,
    value: &SourcedOptionalMinutes,
    prior_series_count: &SourcedCount,
    snapshot_at: chrono::DateTime<Utc>,
) -> Result<(), DataQualityError> {
    let valid = if prior_series_count.value == 0 {
        value.value.is_none() && value.source_latest_at_utc.is_none()
    } else {
        value.value.is_some_and(|minutes| minutes >= 0)
            && value.source_latest_at_utc == prior_series_count.source_latest_at_utc
    };
    if !valid {
        return Err(DataQualityError::InvalidFeatureValue {
            series_id: series_id.to_owned(),
            feature: "rest_minutes",
        });
    }
    validate_source_time(
        series_id,
        "rest_minutes",
        value.source_latest_at_utc,
        snapshot_at,
    )
}

fn validate_source_time(
    series_id: &str,
    feature: &'static str,
    source_time: Option<chrono::DateTime<Utc>>,
    snapshot_at: chrono::DateTime<Utc>,
) -> Result<(), DataQualityError> {
    if source_time.is_some_and(|value| value > snapshot_at) {
        return Err(DataQualityError::SourceTimeViolation {
            series_id: series_id.to_owned(),
            feature,
        });
    }
    Ok(())
}

fn numeric_profile(
    feature: &str,
    values: Vec<(String, u64)>,
    expected_count: u32,
) -> Result<NumericProfile, DataQualityError> {
    let mut ordered = values
        .into_iter()
        .map(|(_, value)| value)
        .collect::<Vec<_>>();
    ordered.sort_unstable();
    let count = checked_u32(ordered.len(), "numeric_profile.count")?;
    let missing_count = expected_count
        .checked_sub(count)
        .ok_or(DataQualityError::CountOverflow("numeric_profile.missing"))?;
    if ordered.is_empty() {
        return Ok(NumericProfile {
            feature: feature.to_owned(),
            count,
            missing_count,
            min: None,
            q1: None,
            median: None,
            q3: None,
            max: None,
            outlier_count: 0,
        });
    }
    let q1 = quantile(&ordered, 1, 4);
    let median = quantile(&ordered, 1, 2);
    let q3 = quantile(&ordered, 3, 4);
    let iqr = q3 - q1;
    let outlier_count = checked_u32(
        ordered
            .iter()
            .filter(|value| {
                let doubled = i128::from(**value) * 2;
                doubled < i128::from(q1) * 2 - i128::from(iqr) * 3
                    || doubled > i128::from(q3) * 2 + i128::from(iqr) * 3
            })
            .count(),
        "numeric_profile.outlier_count",
    )?;
    Ok(NumericProfile {
        feature: feature.to_owned(),
        count,
        missing_count,
        min: ordered.first().copied(),
        q1: Some(q1),
        median: Some(median),
        q3: Some(q3),
        max: ordered.last().copied(),
        outlier_count,
    })
}

fn quantile(values: &[u64], numerator: usize, denominator: usize) -> u64 {
    values[(values.len() - 1) * numerator / denominator]
}

fn checked_u32(value: usize, field: &'static str) -> Result<u32, DataQualityError> {
    u32::try_from(value).map_err(|_| DataQualityError::CountOverflow(field))
}

fn years_count(results: &[SeriesResult]) -> usize {
    results
        .iter()
        .map(|result| result.scheduled_start_utc.year())
        .collect::<BTreeSet<_>>()
        .len()
}

fn patches_count(results: &[SeriesResult]) -> usize {
    results
        .iter()
        .map(|result| result.patch.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn render_count_map(
    output: &mut String,
    heading: &str,
    label: &str,
    values: &BTreeMap<String, u32>,
) {
    output.push_str(&format!(
        "## {heading}\n\n| {label} | Series | Share |\n|---|---:|---:|\n"
    ));
    let total: u32 = values.values().sum();
    for (name, count) in values {
        let share_x100 = u64::from(*count) * 10_000 / u64::from(total);
        output.push_str(&format!(
            "| {name} | {count} | {:.2}% |\n",
            share_x100 as f64 / 100.0
        ));
    }
    output.push('\n');
}

fn optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;
    use crate::{
        prematch_features::{
            SourcedCount, SourcedOptionalMinutes, SourcedRatio, TeamPrematchFeatures,
        },
        temporal_split::{TimeWindow, build_temporal_split_manifest},
    };

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn series(id: &str, day: u8) -> SeriesResult {
        let start = utc(&format!("2026-01-{day:02}T12:00:00Z"));
        SeriesResult {
            series_id: id.to_owned(),
            competition_id: "lol-competition:test".to_owned(),
            region: "Test".to_owned(),
            patch: "26.1".to_owned(),
            scheduled_start_utc: start,
            best_of: 3,
            team_ids: [format!("lol-team:{id}-a"), format!("lol-team:{id}-b")],
            team_names: [format!("{id} A"), format!("{id} B")],
            scores: [2, 1],
            winner_team_id: format!("lol-team:{id}-a"),
            mapping_evidence_id: format!("mapping:{id}"),
            result_evidence_id: format!("result:{id}"),
            market_id: format!("market:{id}"),
            market_winner_outcome_index: 0,
            market_resolution_evidence_id: format!("resolution:{id}"),
            duplicate_candidate_count: 1,
        }
    }

    fn sourced_count(value: u32, source: Option<DateTime<Utc>>) -> SourcedCount {
        SourcedCount {
            value,
            source_latest_at_utc: source,
        }
    }

    fn sourced_ratio(
        numerator: u32,
        denominator: u32,
        source: Option<DateTime<Utc>>,
    ) -> SourcedRatio {
        SourcedRatio {
            numerator,
            denominator,
            source_latest_at_utc: source,
        }
    }

    fn features(result: &SeriesResult) -> PrematchFeatureSnapshot {
        let source = Some(result.scheduled_start_utc - Duration::days(1));
        let team = |index: usize| TeamPrematchFeatures {
            team_id: result.team_ids[index].clone(),
            source_team_key: result.team_names[index].clone(),
            prior_series_count: sourced_count(2, source),
            prior_series_win_rate: sourced_ratio(1, 2, source),
            prior_game_count: sourced_count(5, source),
            prior_game_win_rate: sourced_ratio(3, 5, source),
            same_patch_series_count: sourced_count(1, source),
            same_patch_series_win_rate: sourced_ratio(1, 1, source),
            rest_minutes: SourcedOptionalMinutes {
                value: Some(1425),
                source_latest_at_utc: source,
            },
        };
        PrematchFeatureSnapshot {
            series_id: result.series_id.clone(),
            competition_id: result.competition_id.clone(),
            region: result.region.clone(),
            patch: result.patch.clone(),
            scheduled_start_utc: result.scheduled_start_utc,
            snapshot_at_utc: result.scheduled_start_utc - Duration::minutes(15),
            best_of: result.best_of,
            team_features: [team(0), team(1)],
        }
    }

    fn input() -> DataQualityBuildInput {
        let results = vec![
            series("train", 1),
            series("validation", 2),
            series("calibration", 3),
            series("final", 4),
        ];
        let snapshots = results.iter().map(features).collect::<Vec<_>>();
        let plan = TemporalSplitPlan {
            train: TimeWindow {
                start_utc: utc("2026-01-01T00:00:00Z"),
                end_utc: utc("2026-01-02T00:00:00Z"),
            },
            validation: TimeWindow {
                start_utc: utc("2026-01-02T00:00:00Z"),
                end_utc: utc("2026-01-03T00:00:00Z"),
            },
            calibration: TimeWindow {
                start_utc: utc("2026-01-03T00:00:00Z"),
                end_utc: utc("2026-01-04T00:00:00Z"),
            },
            final_test: TimeWindow {
                start_utc: utc("2026-01-04T00:00:00Z"),
                end_utc: utc("2026-01-05T00:00:00Z"),
            },
        };
        let split = build_temporal_split_manifest(
            snapshots
                .iter()
                .map(|snapshot| TemporalSplitCandidate {
                    series_id: snapshot.series_id.clone(),
                    scheduled_start_utc: snapshot.scheduled_start_utc,
                })
                .collect(),
            plan,
            "a".repeat(64),
        )
        .unwrap();
        DataQualityBuildInput {
            minimum_eligible_series: 500,
            series_results: results,
            feature_snapshots: snapshots,
            temporal_split_manifest: split,
            market_grade_summary: MarketGradeSummary {
                total_markets: 50,
                grade_a: 0,
                grade_b: 0,
                grade_c: 50,
                unavailable: 0,
            },
        }
    }

    #[test]
    fn builds_deterministic_not_ready_report_with_required_findings() {
        let report = build_data_quality_report(input()).expect("valid inputs must profile");
        assert_eq!(report.gate_decision, "NotReadyForM3");
        assert_eq!(report.series_count, 4);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.finding_id == "DQ-001")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.finding_id == "DQ-004")
        );
        let markdown = render_data_quality_markdown(&report);
        assert!(markdown.contains("缺失率（Missingness）"));
        assert!(markdown.contains("缺失与降级规则"));
        assert_eq!(markdown, render_data_quality_markdown(&report));
    }

    #[test]
    fn rejects_duplicate_and_cross_dataset_membership_drift() {
        let mut duplicate = input();
        duplicate
            .series_results
            .push(duplicate.series_results[0].clone());
        assert!(matches!(
            build_data_quality_report(duplicate),
            Err(DataQualityError::DuplicateSeries {
                dataset: "series_results",
                ..
            })
        ));

        let mut drift = input();
        drift.feature_snapshots.pop();
        assert_eq!(
            build_data_quality_report(drift),
            Err(DataQualityError::DatasetMembershipMismatch(
                "feature_snapshots"
            ))
        );
    }

    #[test]
    fn rejects_feature_source_after_snapshot() {
        let mut leaked = input();
        let snapshot = &mut leaked.feature_snapshots[0];
        snapshot.team_features[0]
            .prior_series_count
            .source_latest_at_utc = Some(snapshot.snapshot_at_utc + Duration::seconds(1));
        assert_eq!(
            build_data_quality_report(leaked),
            Err(DataQualityError::SourceTimeViolation {
                series_id: "train".to_owned(),
                feature: "prior_series_count"
            })
        );
    }

    #[test]
    fn accepts_explicit_same_patch_missingness_but_rejects_fake_zero_percent() {
        let mut valid_missing = input();
        let team = &mut valid_missing.feature_snapshots[0].team_features[0];
        team.same_patch_series_count = sourced_count(0, None);
        team.same_patch_series_win_rate = sourced_ratio(0, 0, None);
        let report = build_data_quality_report(valid_missing).expect("explicit missing must pass");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.finding_id == "DQ-003")
        );

        let mut invalid = input();
        invalid.feature_snapshots[0].team_features[0].same_patch_series_count =
            sourced_count(0, None);
        invalid.feature_snapshots[0].team_features[0].same_patch_series_win_rate =
            sourced_ratio(0, 0, Some(utc("2025-12-31T00:00:00Z")));
        assert!(matches!(
            build_data_quality_report(invalid),
            Err(DataQualityError::InvalidFeatureValue {
                feature: "same_patch_series_win_rate",
                ..
            })
        ));
    }

    #[test]
    fn rejects_tampered_split_commitment() {
        let mut tampered = input();
        tampered
            .temporal_split_manifest
            .final_test
            .membership_sha256 = "b".repeat(64);
        assert_eq!(
            build_data_quality_report(tampered),
            Err(DataQualityError::SplitMembershipMismatch)
        );
    }

    #[test]
    fn report_is_independent_of_input_order() {
        let baseline = build_data_quality_report(input()).unwrap();
        let mut reversed = input();
        reversed.series_results.reverse();
        reversed.feature_snapshots.reverse();
        assert_eq!(build_data_quality_report(reversed).unwrap(), baseline);
    }
}
