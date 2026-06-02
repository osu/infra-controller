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

use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Debug, Hash, serde::Deserialize, serde::Serialize)]
pub struct NvueConfigWithHeader {
    pub header: serde_json::Value,
    #[serde(rename = "set")]
    pub config: NvueConfig,
}

impl NvueConfigWithHeader {
    /// Consume `self` and return just the `NvueConfig` inside it.
    pub fn into_nvue_config(self) -> NvueConfig {
        self.config
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn remove_rev_id(&mut self) {
        if let serde_json::Value::Object(header_object) = &mut self.header {
            let _ = header_object.remove("rev-id");
        }
    }
}

#[derive(Clone, Debug, Hash, serde::Deserialize, serde::Serialize)]
pub struct NvueConfig {
    pub bridge: serde_json::Value,
    pub evpn: serde_json::Value,
    pub interface: serde_json::Value,
    pub nve: serde_json::Value,
    pub router: serde_json::Value,
    pub system: serde_json::Value,
    pub vrf: serde_json::Value,
    pub acl: serde_json::Value,
}

impl NvueConfig {
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn u64_hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.hash(&mut h);
        h.finish()
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct NvueRevision {
    // FIXME: Replace this with a more strongly typed inner representation
    revision_json: serde_json::Value,
}

impl NvueRevision {
    pub fn get_revision_id(&self) -> Option<String> {
        dbg!(self);
        if let serde_json::Value::Object(map) = &self.revision_json
            && map.len() == 1
        {
            map.keys().nth(0).cloned()
        } else {
            None
        }
    }
}
