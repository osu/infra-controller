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

use std::sync::Arc;

use carbide_health_metrics::PerObjectMetricsRegistry;
use carbide_ipmi::IPMITool;
use carbide_redfish::libredfish::RedfishClientPool;
use carbide_secrets::credentials::{BmcCredentialType, CredentialKey, CredentialReader};
use component_manager::component_manager::ComponentManager;
use component_manager::compute_tray_manager::{
    Backend as ComputeTrayBackend, ComputeTrayAuthentication, ComputeTrayEndpoint,
    ComputeTrayManager, ComputeTrayResult, ComputeTrayVendor,
};
use db::db_read::PgPoolReader;
use libredfish::Redfish;
use model::component_manager::PowerAction;
use model::machine::Machine;
use sqlx::PgPool;
use state_controller::state_handler::{StateHandlerContextObjects, StateHandlerError};

use crate::config::MachineStateHandlerSiteConfig;
use crate::metrics::MachineMetrics;

pub struct MachineStateHandlerContextObjects {}

impl StateHandlerContextObjects for MachineStateHandlerContextObjects {
    type Services = MachineStateHandlerServices;
    type ObjectMetrics = MachineMetrics;
}

#[derive(Clone)]
pub struct MachineStateHandlerServices {
    pub db_pool: PgPool,
    /// Postgres database pool that can be passed directly to read-only db functions without a
    /// transaction
    pub db_reader: PgPoolReader,
    /// API for interaction with Libredfish
    pub redfish_client_pool: Arc<dyn RedfishClientPool>,
    /// Core's Redfish-backed compute-tray implementation. Standalone hosts and
    /// rack hosts whose configured backend is not RMS always use this path.
    pub core_compute_tray_manager: Arc<dyn ComputeTrayManager>,
    /// The configured component manager. Rack-associated hosts use its compute
    /// backend when that backend is RMS.
    pub component_manager: Option<Arc<ComponentManager>>,
    /// Credential source used to build compute-tray backend endpoints.
    pub credential_reader: Arc<dyn CredentialReader>,
    /// An implementation of the IPMITool that understands how to reboot a machine
    pub ipmi_tool: Arc<dyn IPMITool>,
    /// Configuration used by MachineStateHandler.
    pub site_config: Arc<MachineStateHandlerSiteConfig>,
    /// Shared registry backing the generic per-object health metrics.
    pub per_object_metrics_registry: Arc<PerObjectMetricsRegistry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComputeTrayRoute {
    Core,
    ConfiguredRms,
}

fn compute_tray_route(
    rack_associated: bool,
    configured_backend: Option<ComputeTrayBackend>,
) -> ComputeTrayRoute {
    if rack_associated && configured_backend == Some(ComputeTrayBackend::Rms) {
        ComputeTrayRoute::ConfiguredRms
    } else {
        ComputeTrayRoute::Core
    }
}

fn validate_compute_tray_power_result(
    backend_name: &str,
    machine_id: &str,
    expected_bmc_ip: std::net::IpAddr,
    results: Vec<ComputeTrayResult>,
) -> Result<(), String> {
    if results.len() != 1 {
        return Err(format!(
            "compute-tray backend {backend_name} returned {} results for one requested machine {machine_id}",
            results.len()
        ));
    }

    let Some(result) = results.into_iter().next() else {
        return Err(format!(
            "compute-tray backend {backend_name} returned no result for machine {machine_id}"
        ));
    };
    if result.bmc_ip != expected_bmc_ip {
        return Err(format!(
            "compute-tray backend {backend_name} returned a result for unexpected BMC {} instead of machine {machine_id} BMC {expected_bmc_ip}",
            result.bmc_ip
        ));
    }
    if !result.success {
        return Err(format!(
            "compute-tray backend {backend_name} power control failed for {machine_id}: {}",
            result.error.unwrap_or_else(|| "unknown error".into())
        ));
    }

    Ok(())
}

impl MachineStateHandlerServices {
    pub fn compute_tray_manager_for(&self, machine: &Machine) -> Arc<dyn ComputeTrayManager> {
        match compute_tray_route(
            machine.rack_id.is_some(),
            self.component_manager
                .as_ref()
                .map(|component_manager| component_manager.compute_tray.backend()),
        ) {
            ComputeTrayRoute::Core => self.core_compute_tray_manager.clone(),
            ComputeTrayRoute::ConfiguredRms => self.component_manager.as_ref().map_or_else(
                || self.core_compute_tray_manager.clone(),
                |component_manager| component_manager.compute_tray.clone(),
            ),
        }
    }

    pub async fn compute_tray_endpoint(
        &self,
        machine: &Machine,
        backend: ComputeTrayBackend,
    ) -> Result<ComputeTrayEndpoint, StateHandlerError> {
        let bmc_ip = machine
            .bmc_info
            .ip
            .ok_or_else(|| StateHandlerError::MissingData {
                object_id: machine.id.to_string(),
                missing: "BMC IP address (bmc_info.ip)",
            })?;
        let bmc_mac = machine
            .bmc_info
            .mac
            .ok_or_else(|| StateHandlerError::MissingData {
                object_id: machine.id.to_string(),
                missing: "BMC MAC address (bmc_info.mac)",
            })?;
        let bmc_credential_key = CredentialKey::BmcCredentials {
            credential_type: BmcCredentialType::BmcRoot {
                bmc_mac_address: bmc_mac,
            },
        };
        let authentication = if backend == ComputeTrayBackend::Core {
            // Preserve Core's existing Redfish-pool behavior: the pool owns
            // credential/session lookup and test pools may intentionally
            // accept the key without a materialized secret.
            ComputeTrayAuthentication::CredentialKey(bmc_credential_key)
        } else {
            let bmc_credentials = match self
                .credential_reader
                .get_credentials(&bmc_credential_key)
                .await
                .map_err(|error| {
                    StateHandlerError::GenericError(eyre::eyre!(
                        "failed to load BMC credentials for {}: {}",
                        machine.id,
                        error
                    ))
                })? {
                Some(credentials) => credentials,
                None => {
                    let sitewide_credential_key = CredentialKey::BmcCredentials {
                        credential_type: BmcCredentialType::SiteWideRoot,
                    };
                    self.credential_reader
                        .get_credentials(&sitewide_credential_key)
                        .await
                        .map_err(|error| {
                            StateHandlerError::GenericError(eyre::eyre!(
                                "failed to load site-wide BMC credentials for {}: {}",
                                machine.id,
                                error
                            ))
                        })?
                        .ok_or_else(|| StateHandlerError::MissingData {
                            object_id: machine.id.to_string(),
                            missing: "per-BMC or site-wide BMC credentials",
                        })?
                }
            };
            ComputeTrayAuthentication::Credentials(bmc_credentials)
        };

        let vendor = match machine.bmc_vendor() {
            bmc_vendor::BMCVendor::Dell => ComputeTrayVendor::Dell,
            bmc_vendor::BMCVendor::Hpe => ComputeTrayVendor::Hpe,
            bmc_vendor::BMCVendor::Lenovo => ComputeTrayVendor::Lenovo,
            bmc_vendor::BMCVendor::LenovoAMI => ComputeTrayVendor::LenovoAmi,
            bmc_vendor::BMCVendor::Supermicro => ComputeTrayVendor::Supermicro,
            bmc_vendor::BMCVendor::Nvidia => ComputeTrayVendor::Nvidia,
            _ => ComputeTrayVendor::Unknown,
        };

        Ok(ComputeTrayEndpoint {
            vendor,
            bmc_ip,
            bmc_port: machine.bmc_info.port,
            authentication,
        })
    }

    pub async fn power_control(
        &self,
        machine: &Machine,
        action: PowerAction,
    ) -> Result<(), StateHandlerError> {
        let backend = self.compute_tray_manager_for(machine);
        self.power_control_with_manager(machine, backend.as_ref(), action)
            .await
    }

    pub async fn power_control_with_manager(
        &self,
        machine: &Machine,
        backend: &dyn ComputeTrayManager,
        action: PowerAction,
    ) -> Result<(), StateHandlerError> {
        let endpoint = self
            .compute_tray_endpoint(machine, backend.backend())
            .await?;
        let results = backend
            .power_control(std::slice::from_ref(&endpoint), action)
            .await
            .map_err(|error| {
                StateHandlerError::GenericError(eyre::eyre!(
                    "compute-tray backend {} power control failed: {}",
                    backend.name(),
                    error
                ))
            })?;
        validate_compute_tray_power_result(
            backend.name(),
            &machine.id.to_string(),
            endpoint.bmc_ip,
            results,
        )
        .map_err(|error| StateHandlerError::GenericError(eyre::eyre!(error)))
    }

    pub async fn create_redfish_client_from_machine(
        &self,
        machine: &Machine,
    ) -> Result<Box<dyn Redfish>, StateHandlerError> {
        let addr = machine
            .bmc_addr()
            .ok_or_else(|| StateHandlerError::MissingData {
                object_id: machine.id.to_string(),
                missing: "BMC Endpoint Information (bmc_info.ip)",
            })?;
        let bmc_access_info = db::machine_interface::lookup_bmc_access_info(
            &self.db_pool,
            addr.ip(),
            Some(addr.port()),
        )
        .await?;
        self.redfish_client_pool
            .client_by_info(&bmc_access_info)
            .await
            .map_err(StateHandlerError::from)
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::*;

    #[test]
    fn compute_tray_route_uses_rms_only_for_rack_machines() {
        let cases = [
            (
                "standalone with RMS configured",
                false,
                Some(ComputeTrayBackend::Rms),
                ComputeTrayRoute::Core,
            ),
            (
                "rack with RMS configured",
                true,
                Some(ComputeTrayBackend::Rms),
                ComputeTrayRoute::ConfiguredRms,
            ),
            (
                "rack with Core configured",
                true,
                Some(ComputeTrayBackend::Core),
                ComputeTrayRoute::Core,
            ),
            (
                "rack with mock configured",
                true,
                Some(ComputeTrayBackend::Mock),
                ComputeTrayRoute::Core,
            ),
            (
                "rack without component manager",
                true,
                None,
                ComputeTrayRoute::Core,
            ),
        ];

        for (scenario, rack_associated, backend, expected) in cases {
            assert_eq!(
                compute_tray_route(rack_associated, backend),
                expected,
                "{scenario}"
            );
        }
    }

    #[test]
    fn compute_tray_power_result_requires_one_matching_success() {
        let expected_ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));
        let other_ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 11));
        let result = |bmc_ip, success, error: Option<&str>| ComputeTrayResult {
            bmc_ip,
            success,
            error: error.map(str::to_string),
        };
        let cases = [
            (
                "one matching success",
                vec![result(expected_ip, true, None)],
                true,
            ),
            ("empty result", vec![], false),
            (
                "duplicate result",
                vec![
                    result(expected_ip, true, None),
                    result(expected_ip, true, None),
                ],
                false,
            ),
            ("unexpected BMC", vec![result(other_ip, true, None)], false),
            (
                "matching backend failure",
                vec![result(expected_ip, false, Some("rejected"))],
                false,
            ),
        ];

        for (scenario, results, expected) in cases {
            assert_eq!(
                validate_compute_tray_power_result("test", "Machine:test", expected_ip, results,)
                    .is_ok(),
                expected,
                "{scenario}"
            );
        }
    }
}
