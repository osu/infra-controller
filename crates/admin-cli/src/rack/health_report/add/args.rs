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

use carbide_uuid::rack::RackId;
use clap::{ArgGroup, Parser};

use crate::machine::HealthReportTemplates;

#[derive(Parser, Debug)]
#[clap(group(ArgGroup::new("health_report_source").required(true).args(&["health_report", "template"])))]
#[command(after_long_help = "\
EXAMPLES:

Add a health report source from a predefined template:
    $ nico-admin-cli rack health-report add rack-123 --template internal-maintenance \
    --message \"Firmware upgrade in progress\"

Add a health report source from raw JSON and replace existing reports:
    $ nico-admin-cli rack health-report add rack-123 \
    --health-report '{\"source\":\"admin-cli\",\"observed_at\":null,\
    \"successes\":[],\"alerts\":[]}' --replace

Preview the report without sending it:
    $ nico-admin-cli rack health-report add rack-123 --template degraded --print-only

")]
/// Arguments for adding a health report source to a rack.
pub struct Args {
    /// Rack whose health reports will be updated.
    pub rack_id: RackId,
    /// Raw report payload; mutually exclusive with `--template`.
    #[clap(
        long,
        help = "New health report as JSON; mutually exclusive with --template"
    )]
    pub health_report: Option<String>,
    /// Predefined report template; mutually exclusive with `--health-report`.
    #[clap(
        long,
        help = "Predefined template name; mutually exclusive with --health-report"
    )]
    pub template: Option<HealthReportTemplates>,
    /// Optional message used only with a predefined template.
    #[clap(
        long,
        requires = "template",
        conflicts_with = "health_report",
        help = "Message to fill in the template"
    )]
    pub message: Option<String>,
    /// Replace all existing sources instead of merging this report.
    #[clap(long, help = "Replace all other health reports with this source")]
    pub replace: bool,
    /// Render the report without sending it to the API.
    #[clap(long, help = "Print the report without sending it to the API")]
    pub print_only: bool,
}
