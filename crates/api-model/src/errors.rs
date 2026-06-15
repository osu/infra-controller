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
use carbide_uuid::machine::MachineId;
use serde::{Deserialize, Serialize};

use crate::hardware_info::HardwareInfoError;

/// Operator-facing error schema suitable for logs, API metadata, and UI display.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OperatorErrorSchema {
    /// Stable identifier for alerting and centralized log filtering.
    pub error_code: String,
    /// Human-readable problem description.
    pub text: String,
    /// Suggested operator action when NICo knows a specific mitigation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mitigation: Option<String>,
}

impl OperatorErrorSchema {
    pub fn new(
        error_code: impl Into<String>,
        text: impl Into<String>,
        mitigation: Option<String>,
    ) -> Self {
        Self {
            error_code: error_code.into(),
            text: text.into(),
            mitigation,
        }
    }

    pub fn mitigation_for_log(&self) -> &str {
        self.mitigation.as_deref().unwrap_or("")
    }
}

/// Errors specifically for the (eventual) models crate
#[derive(thiserror::Error, Debug)]
pub enum ModelError {
    #[error("Failed to map device to dpu: {0}")]
    DpuMappingError(String),
    #[error("DPU {0} is missing from host snapshot")]
    MissingDpu(MachineId),
    #[error("Database type conversion error: {0}")]
    DatabaseTypeConversionError(String),
    #[error("Argument is missing in input: {0}")]
    MissingArgument(&'static str),
    #[error("Hardware info error: {0}")]
    HardwareInfo(#[from] HardwareInfoError),
    #[error("Argument is invalid: {0}")]
    InvalidArgument(String),
}

pub type ModelResult<T> = Result<T, ModelError>;
