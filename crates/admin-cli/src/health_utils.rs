/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use ::rpc::admin_cli::OutputFormat;
use ::rpc::forge::{self as forgerpc};
use ::rpc::health::HealthReport as RpcHealthReport;
use prettytable::{Table, row};

use crate::errors::{CarbideCliError, CarbideCliResult};
use crate::machine::health_report::cmd::get_empty_template;
use crate::machine::{HealthReportTemplates, get_health_report};

/// Display a list of health report entries.
pub fn display_health_reports(
    entries: Vec<forgerpc::HealthReportEntry>,
    output_format: OutputFormat,
) -> CarbideCliResult<()> {
    let mut rows = vec![];
    for entry in entries {
        let report = entry.report.ok_or(CarbideCliError::GenericError(
            "missing response".to_string(),
        ))?;
        let mode = match forgerpc::HealthReportApplyMode::try_from(entry.mode)
            .map_err(|_| CarbideCliError::GenericError("invalid response".to_string()))?
        {
            forgerpc::HealthReportApplyMode::Merge => "Merge",
            forgerpc::HealthReportApplyMode::Replace => "Replace",
        };
        rows.push((report, mode));
    }
    match output_format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(
                &rows
                    .into_iter()
                    .map(|r| {
                        serde_json::json!({
                            "report": r.0,
                            "mode": r.1,
                        })
                    })
                    .collect::<Vec<_>>(),
            )?
        ),
        _ => {
            let mut table = Table::new();
            table.set_titles(row!["Report", "Mode"]);
            for row in rows {
                table.add_row(row![serde_json::to_string(&row.0)?, row.1]);
            }
            table.printstd();
        }
    }
    Ok(())
}

/// Return an operator-facing status for an aggregate health report.
pub fn aggregate_health_status(health: Option<&RpcHealthReport>) -> &'static str {
    match health {
        Some(health) if health.alerts.is_empty() => "Healthy",
        Some(_) => "Unhealthy",
        None => "Unknown",
    }
}

/// Format aggregate health alerts for human-readable component output.
pub fn format_health_alerts(health: Option<&RpcHealthReport>) -> String {
    let Some(health) = health else {
        return "N/A".to_string();
    };
    if health.alerts.is_empty() {
        return "None".to_string();
    }

    health
        .alerts
        .iter()
        .map(|alert| {
            let target = alert
                .target
                .as_deref()
                .map(|target| format!(" [Target: {target}]"))
                .unwrap_or_default();
            let classifications = if alert.classifications.is_empty() {
                String::new()
            } else {
                format!(" [Classifications: {}]", alert.classifications.join(", "))
            };
            format!("{}{target}: {}{classifications}", alert.id, alert.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format health report source names in response order.
pub fn format_health_sources(sources: &[forgerpc::HealthSourceOrigin]) -> String {
    if sources.is_empty() {
        "None".to_string()
    } else {
        sources
            .iter()
            .map(|source| source.source.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Resolve a health report from either a template or raw JSON.
pub fn resolve_health_report(
    template: Option<HealthReportTemplates>,
    health_report_json: Option<String>,
    message: Option<String>,
) -> CarbideCliResult<health_report::HealthReport> {
    if let Some(template) = template {
        Ok(get_health_report(template, message))
    } else if let Some(json) = health_report_json {
        serde_json::from_str::<health_report::HealthReport>(&json)
            .map_err(CarbideCliError::JsonError)
    } else {
        Err(CarbideCliError::GenericError(
            "Either health_report or template name must be provided.".to_string(),
        ))
    }
}

/// Print the empty health report template.
pub fn print_empty_template() {
    println!(
        "{}",
        serde_json::to_string_pretty(&get_empty_template()).unwrap()
    );
}

#[cfg(test)]
mod tests {
    use carbide_test_support::Outcome::*;
    use carbide_test_support::{Case, check_cases};
    use rpc::forge::{HealthReportApplyMode, HealthSourceOrigin};
    use rpc::health::{HealthProbeAlert, HealthReport};

    use super::*;

    fn healthy_report() -> HealthReport {
        HealthReport {
            source: "aggregate-health".to_string(),
            triggered_by: None,
            observed_at: None,
            successes: vec![],
            alerts: vec![],
        }
    }

    fn unhealthy_report() -> HealthReport {
        HealthReport {
            alerts: vec![HealthProbeAlert {
                id: "FanFailure".to_string(),
                target: Some("fan-1".to_string()),
                in_alert_since: None,
                message: "Fan failed".to_string(),
                tenant_message: None,
                classifications: vec!["Hardware".to_string()],
            }],
            ..healthy_report()
        }
    }

    // aggregate_health_status distinguishes missing, healthy, and unhealthy
    // aggregate reports.
    #[test]
    fn aggregate_health_status_covers_report_states() {
        check_cases(
            [
                Case {
                    scenario: "missing report is unknown",
                    input: None,
                    expect: Yields("Unknown".to_string()),
                },
                Case {
                    scenario: "report without alerts is healthy",
                    input: Some(healthy_report()),
                    expect: Yields("Healthy".to_string()),
                },
                Case {
                    scenario: "report with alerts is unhealthy",
                    input: Some(unhealthy_report()),
                    expect: Yields("Unhealthy".to_string()),
                },
            ],
            |health| -> Result<String, ()> {
                Ok(aggregate_health_status(health.as_ref()).to_string())
            },
        );
    }

    // format_health_alerts covers missing, healthy, and unhealthy reports.
    #[test]
    fn format_health_alerts_covers_report_states() {
        check_cases(
            [
                Case {
                    scenario: "missing report",
                    input: None,
                    expect: Yields("N/A".to_string()),
                },
                Case {
                    scenario: "healthy report",
                    input: Some(healthy_report()),
                    expect: Yields("None".to_string()),
                },
                Case {
                    scenario: "unhealthy report",
                    input: Some(unhealthy_report()),
                    expect: Yields(
                        "FanFailure [Target: fan-1]: Fan failed [Classifications: Hardware]"
                            .to_string(),
                    ),
                },
            ],
            |health| -> Result<String, ()> { Ok(format_health_alerts(health.as_ref())) },
        );
    }

    /// format_health_sources renders source names in response order.
    #[test]
    fn format_health_sources_renders_source_names() {
        let sources = [
            HealthSourceOrigin {
                mode: HealthReportApplyMode::Merge as i32,
                source: "rack-controller".to_string(),
            },
            HealthSourceOrigin {
                mode: HealthReportApplyMode::Replace as i32,
                source: "internal-maintenance".to_string(),
            },
        ];

        assert_eq!(
            format_health_sources(&sources),
            "rack-controller\ninternal-maintenance"
        );
        assert_eq!(format_health_sources(&[]), "None");
    }
}
