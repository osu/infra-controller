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

use crate::client::NvueClientError;

#[derive(Clone, Debug, Hash, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct NvueConfig {
    pub bridge: Option<serde_json::Value>,
    pub evpn: Option<serde_json::Value>,
    pub interface: Option<serde_json::Value>,
    pub nve: Option<serde_json::Value>,
    pub router: Option<serde_json::Value>,
    pub system: Option<serde_json::Value>,
    pub vrf: Option<serde_json::Value>,
    pub acl: Option<serde_json::Value>,
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

#[derive(Clone, Debug, Hash, serde::Deserialize, serde::Serialize)]
#[serde(try_from = "NvueConfigKeyValueSequence")]
pub struct NvueConfigWithHeader {
    pub header: NvueConfigHeader,
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
}

#[derive(Clone, Debug, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NvueConfigHeader {
    pub model: Option<String>,
    pub nvue_api_version: Option<String>,
    // Ideally rev_id would also be Option<String>, but there's a serde_yaml
    // bug that prevents the value from being coereced to String when
    // this NvueConfigHeader is inside a container (I think because of the
    // `transparent` attribute on `NvueConfigKeyValueSequence`?)
    pub rev_id: Option<serde_json::Value>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
enum NvueConfigKeyValueEntry {
    Header { header: NvueConfigHeader },
    SetConfig { set: NvueConfig },
}

/// This models the structure of the startup config file, which uses
/// the sequence-of-single-key-maps pattern at its top level. This is
/// used as an intermediate deserialization step before converting to
/// `NvueConfigWithHeader`.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
struct NvueConfigKeyValueSequence {
    entries: Vec<NvueConfigKeyValueEntry>,
}

impl TryFrom<NvueConfigKeyValueSequence> for NvueConfigWithHeader {
    type Error = NvueClientError;

    fn try_from(value: NvueConfigKeyValueSequence) -> Result<Self, Self::Error> {
        use NvueConfigKeyValueEntry::*;

        let mut header_entry = None;
        let mut config_entry = None;

        for entry in value.entries.into_iter() {
            match entry {
                Header { header } => {
                    let previous = header_entry.replace(header);
                    if previous.is_some() {
                        return Err(NvueClientError::SchemaMismatch(
                            "Found more than one 'header' object in sequence",
                        ));
                    }
                }
                SetConfig { set } => {
                    let previous = config_entry.replace(set);
                    if previous.is_some() {
                        return Err(NvueClientError::SchemaMismatch(
                            "Found more than one 'set' object in sequence",
                        ));
                    }
                }
            }
        }

        match (header_entry, config_entry) {
            (Some(header), Some(config)) => Ok(NvueConfigWithHeader { header, config }),
            (None, _) => Err(NvueClientError::SchemaMismatch(
                "No 'header' object found in sequence",
            )),
            (_, None) => Err(NvueClientError::SchemaMismatch(
                "No 'set' object found in sequence",
            )),
        }
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
