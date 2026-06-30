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

use std::str::FromStr;

use carbide_uuid::power_shelf::PowerShelfId;
use color_eyre::Result;
use prettytable::{Table, row};
use rpc::admin_cli::OutputFormat;
use rpc::forge::PowerShelf;

use super::args::Args;
use crate::cfg::runtime::RuntimeConfig;
use crate::errors::CarbideCliResult;
use crate::health_utils;
use crate::rpc::ApiClient;

/// Build power-shelf output with distinct hardware and aggregate health fields.
fn build_table(shelves: &[PowerShelf]) -> Table {
    let mut table = Table::new();
    table.set_titles(row![
        "ID",
        "Name",
        "Metadata Name",
        "Capacity(W)",
        "Voltage(V)",
        "Power State",
        "Hardware Health",
        "Aggregate Health",
        "Health Alerts",
        "Health Reports",
        "State",
        "BMC IP",
        "BMC MAC",
        "BMC Interface ID"
    ]);

    for shelf in shelves {
        let metadata_name = shelf
            .metadata
            .as_ref()
            .map(|m| m.name.as_str())
            .unwrap_or("N/A");
        let status = shelf.status.as_ref();
        let bmc_ip = shelf
            .bmc_info
            .as_ref()
            .and_then(|b| b.ip.clone())
            .unwrap_or_else(|| "N/A".to_string());
        let bmc_mac = shelf
            .bmc_info
            .as_ref()
            .and_then(|b| b.mac.clone())
            .unwrap_or_else(|| "N/A".to_string());
        let bmc_interface_id = shelf
            .bmc_info
            .as_ref()
            .and_then(|b| b.machine_interface_id.map(|id| id.to_string()))
            .unwrap_or_else(|| "N/A".to_string());

        table.add_row(row![
            shelf
                .id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            shelf
                .config
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or("N/A"),
            metadata_name,
            shelf
                .config
                .as_ref()
                .and_then(|c| c.capacity)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            shelf
                .config
                .as_ref()
                .and_then(|c| c.voltage)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            status
                .and_then(|status| status.power_state.as_deref())
                .unwrap_or("N/A"),
            status
                .and_then(|status| status.health_status.as_deref())
                .unwrap_or("N/A"),
            health_utils::aggregate_health_status(
                status.and_then(|status| status.health.as_ref()),
            ),
            health_utils::format_health_alerts(
                status.and_then(|status| status.health.as_ref()),
            ),
            health_utils::format_health_sources(
                status
                    .map(|status| status.health_sources.as_slice())
                    .unwrap_or_default(),
            ),
            shelf.controller_state,
            bmc_ip,
            bmc_mac,
            bmc_interface_id,
        ]);
    }

    table
}

/// Render power shelves in the requested output format.
pub fn show_power_shelves(
    power_shelves: Vec<PowerShelf>,
    output_format: OutputFormat,
) -> Result<()> {
    match output_format {
        OutputFormat::AsciiTable => {
            build_table(&power_shelves).printstd();
        }
        OutputFormat::Json => {
            println!("JSON output not supported for PowerShelf (protobuf type)");
            println!("Use ASCII table format instead.");
        }
        OutputFormat::Yaml => {
            println!("YAML output not supported for PowerShelf (protobuf type)");
            println!("Use ASCII table format instead.");
        }
        OutputFormat::Csv => {
            build_table(&power_shelves).to_csv(std::io::stdout()).ok();
        }
    }

    Ok(())
}

pub async fn handle_show(
    args: Args,
    api_client: &ApiClient,
    config: &RuntimeConfig,
) -> CarbideCliResult<()> {
    let power_shelves = match args.identifier {
        Some(id) if !id.is_empty() => match PowerShelfId::from_str(&id) {
            Ok(power_shelf_id) => {
                api_client
                    .get_one_power_shelf(power_shelf_id)
                    .await?
                    .power_shelves
            }
            Err(_) => {
                // Fall back to name-based lookup
                let query = rpc::forge::PowerShelfQuery {
                    name: Some(id),
                    power_shelf_id: None,
                };
                api_client.0.find_power_shelves(query).await?.power_shelves
            }
        },
        _ => {
            let filter = rpc::forge::PowerShelfSearchFilter::default();
            api_client
                .get_all_power_shelves(filter, config.page_size)
                .await?
                .power_shelves
        }
    };

    show_power_shelves(power_shelves, config.format).ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use rpc::forge::{HealthReportApplyMode, HealthSourceOrigin, PowerShelf, PowerShelfStatus};
    use rpc::health::{HealthProbeAlert, HealthReport};

    use super::*;

    fn table_to_string(table: &Table) -> String {
        let mut bytes = Vec::new();
        table.print(&mut bytes).expect("table should render");
        String::from_utf8(bytes).expect("table output should be UTF-8")
    }

    /// Power shelf output distinguishes legacy hardware health from aggregate
    /// health and includes aggregate alert details and report sources.
    #[test]
    fn table_renders_aggregate_health() {
        let shelf = PowerShelf {
            id: Some(
                "ps100htjtiaehv1n5vh67tbmqq4eabcjdng40f7jupsadbedhruh6rag1l0"
                    .parse()
                    .unwrap(),
            ),
            status: Some(PowerShelfStatus {
                health_status: Some("ok".to_string()),
                health: Some(HealthReport {
                    source: "power-shelf-aggregate-health".to_string(),
                    triggered_by: None,
                    observed_at: None,
                    successes: vec![],
                    alerts: vec![HealthProbeAlert {
                        id: "PowerSupplyFailure".to_string(),
                        target: Some("psu-1".to_string()),
                        in_alert_since: None,
                        message: "Power supply failed".to_string(),
                        tenant_message: None,
                        classifications: vec!["Hardware".to_string()],
                    }],
                }),
                health_sources: vec![HealthSourceOrigin {
                    mode: HealthReportApplyMode::Merge as i32,
                    source: "operator-override".to_string(),
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let rendered = table_to_string(&build_table(&[shelf]));
        assert!(rendered.contains("Hardware Health"));
        assert!(rendered.contains("Aggregate Health"));
        assert!(rendered.contains("ok"));
        assert!(rendered.contains("Unhealthy"));
        assert!(rendered.contains("PowerSupplyFailure"));
        assert!(rendered.contains("Classifications: Hardware"));
        assert!(rendered.contains("operator-override"));
    }
}
