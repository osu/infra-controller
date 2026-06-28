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

use ::rpc::forge::{self as rpc, InsertRackHealthReportRequest};
use carbide_uuid::rack::RackId;
use color_eyre::eyre::WrapErr;

use super::args::Args;
use crate::errors::CarbideCliResult;
use crate::health_utils;
use crate::rpc::ApiClient;

pub async fn add(api_client: &ApiClient, args: Args) -> CarbideCliResult<()> {
    let report =
        health_utils::resolve_health_report(args.template, args.health_report, args.message)
            .wrap_err("Failed to resolve the rack health report")?;

    if args.print_only {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .wrap_err("Failed to serialize the rack health report preview")?
        );
        return Ok(());
    }

    let context = format!("Failed to insert a health report for rack {}", args.rack_id);
    api_client
        .0
        .insert_rack_health_report(build_insert_request(args.rack_id, report, args.replace))
        .await
        .wrap_err(context)?;

    Ok(())
}

fn build_insert_request(
    rack_id: RackId,
    report: ::health_report::HealthReport,
    replace: bool,
) -> InsertRackHealthReportRequest {
    InsertRackHealthReportRequest {
        rack_id: Some(rack_id),
        health_report_entry: Some(rpc::HealthReportEntry {
            report: Some(report.into()),
            mode: if replace {
                rpc::HealthReportApplyMode::Replace
            } else {
                rpc::HealthReportApplyMode::Merge
            } as i32,
        }),
    }
}

#[cfg(test)]
mod tests {
    use carbide_test_support::Outcome::*;
    use carbide_test_support::{Case, check_cases};

    use super::*;

    // build_insert_request preserves the rack/report payload and maps the
    // replace flag to the corresponding API apply mode.
    #[test]
    fn build_insert_request_maps_apply_mode_and_payload() {
        check_cases(
            [
                Case {
                    scenario: "merge with existing reports",
                    input: false,
                    expect: Yields((
                        "rack-123".to_string(),
                        "smoke-report".to_string(),
                        rpc::HealthReportApplyMode::Merge as i32,
                    )),
                },
                Case {
                    scenario: "replace existing reports",
                    input: true,
                    expect: Yields((
                        "rack-123".to_string(),
                        "smoke-report".to_string(),
                        rpc::HealthReportApplyMode::Replace as i32,
                    )),
                },
            ],
            |replace| -> Result<(String, String, i32), ()> {
                let request = build_insert_request(
                    RackId::new("rack-123"),
                    ::health_report::HealthReport::empty("smoke-report".to_string()),
                    replace,
                );
                let entry = request
                    .health_report_entry
                    .expect("request should contain a health report entry");
                Ok((
                    request
                        .rack_id
                        .expect("request should contain a rack ID")
                        .to_string(),
                    entry.report.expect("entry should contain a report").source,
                    entry.mode,
                ))
            },
        );
    }
}
