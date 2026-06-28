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
use ::rpc::forge::ListRackHealthReportsRequest;
use color_eyre::eyre::WrapErr;

use super::args::Args;
use crate::errors::CarbideCliResult;
use crate::health_utils;
use crate::rpc::ApiClient;

/// List and render the health report sources for a rack.
pub async fn show(
    api_client: &ApiClient,
    args: Args,
    format: OutputFormat,
) -> CarbideCliResult<()> {
    let context = format!(
        "while attempting to list health reports for rack {}",
        args.rack_id
    );
    let response = api_client
        .0
        .list_rack_health_reports(ListRackHealthReportsRequest {
            rack_id: Some(args.rack_id),
        })
        .await
        .wrap_err(context)?;
    health_utils::display_health_reports(response.health_report_entries, format)
        .wrap_err("while attempting to display rack health reports")?;
    Ok(())
}
