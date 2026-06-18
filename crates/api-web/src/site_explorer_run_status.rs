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

use rpc::site_explorer::SiteExplorerLastRun;

fn display_error(run: &SiteExplorerLastRun) -> String {
    match run.failure_category.as_deref() {
        Some("missing_credentials" | "set_credentials") => {
            "Site Explorer credentials are missing or invalid".to_string()
        }
        Some("secrets_engine") => "Site Explorer could not access credentials".to_string(),
        _ => run.error.clone().unwrap_or_default(),
    }
}

pub(crate) struct SiteExplorerLastRunDisplay {
    pub(crate) status_label: &'static str,
    pub(crate) status_class: &'static str,
    pub(crate) started_at: String,
    pub(crate) finished_at: String,
    pub(crate) endpoint_explorations: i64,
    pub(crate) endpoint_explorations_success: i64,
    pub(crate) endpoint_explorations_failed: i64,
    pub(crate) failure_category: String,
    pub(crate) last_successful_finished_at: String,
    pub(crate) last_failed_finished_at: String,
    pub(crate) error: String,
}

impl From<&SiteExplorerLastRun> for SiteExplorerLastRunDisplay {
    fn from(run: &SiteExplorerLastRun) -> Self {
        let (status_label, status_class) = if run.success {
            ("Success", "success")
        } else {
            ("Failed", "error")
        };
        Self {
            status_label,
            status_class,
            started_at: run.started_at.clone(),
            finished_at: run.finished_at.clone(),
            endpoint_explorations: run.endpoint_explorations,
            endpoint_explorations_success: run.endpoint_explorations_success,
            endpoint_explorations_failed: run.endpoint_explorations_failed,
            failure_category: run.failure_category.clone().unwrap_or_default(),
            last_successful_finished_at: run
                .last_successful_finished_at
                .clone()
                .unwrap_or_else(|| "Never".to_string()),
            last_failed_finished_at: run
                .last_failed_finished_at
                .clone()
                .unwrap_or_else(|| "Never".to_string()),
            error: display_error(run),
        }
    }
}
