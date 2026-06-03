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

//! Handler for SwitchControllerState::Maintenance.

use carbide_rack::rack_manager_error;
use carbide_uuid::switch::SwitchId;
use db::switch as db_switch;
use forge_secrets::credentials::{
    BmcCredentialType, CredentialKey, CredentialManager, Credentials,
};
use librms::protos::rack_manager as rms;
use mac_address::MacAddress;
use model::switch::{Switch, SwitchControllerState, SwitchMaintenanceOperation};
use sqlx::PgPool;
use state_controller::state_handler::{
    StateHandlerContext, StateHandlerError, StateHandlerOutcome,
};

use crate::context::SwitchStateHandlerContextObjects;

const SWITCH_BMC_PORT: u32 = 443;

/// Handles the Maintenance state for a switch, dispatching on the requested
/// operation (`PowerOn` / `PowerOff` / `Reset`).
pub async fn handle_maintenance(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    let operation = match &state.controller_state.value {
        SwitchControllerState::Maintenance { operation } => *operation,
        _ => unreachable!("handle_maintenance called with non-Maintenance state"),
    };

    match operation {
        SwitchMaintenanceOperation::PowerOn => handle_power_on(switch_id, state, ctx).await,
        SwitchMaintenanceOperation::PowerOff => handle_power_off(switch_id, state, ctx).await,
        SwitchMaintenanceOperation::Reset => handle_reset(switch_id, state, ctx).await,
    }
}

async fn handle_power_on(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    tracing::info!(switch_id = %switch_id, "Switch maintenance: PowerOn");
    invoke_rms_power_operation(
        switch_id,
        state,
        ctx,
        rms::PowerOperation::On,
        "PowerOn",
        SwitchControllerState::ready(),
    )
    .await
}

async fn handle_power_off(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    tracing::info!(switch_id = %switch_id, "Switch maintenance: PowerOff");
    invoke_rms_power_operation(
        switch_id,
        state,
        ctx,
        rms::PowerOperation::Off,
        "PowerOff",
        SwitchControllerState::ready_off(),
    )
    .await
}

async fn handle_reset(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    tracing::info!(switch_id = %switch_id, "Switch maintenance: Reset");
    invoke_rms_power_operation(
        switch_id,
        state,
        ctx,
        rms::PowerOperation::Reset,
        "Reset",
        SwitchControllerState::ready(),
    )
    .await
}

async fn invoke_rms_power_operation(
    switch_id: &SwitchId,
    state: &Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
    operation: rms::PowerOperation,
    operation_label: &'static str,
    success_state: SwitchControllerState,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    let Some(rms_client) = ctx.services.rms_client.as_ref() else {
        return finish_maintenance_with_error(
            switch_id,
            ctx,
            format!(
                "Switch {} maintenance ({}): RMS client not configured",
                switch_id, operation_label
            ),
        )
        .await;
    };

    let Some(rack_id) = state.rack_id.as_ref() else {
        return finish_maintenance_with_error(
            switch_id,
            ctx,
            format!(
                "Switch {} maintenance ({}): switch has no rack association",
                switch_id, operation_label
            ),
        )
        .await;
    };

    let device = match build_switch_node_info(
        switch_id,
        state,
        rack_id.to_string(),
        &ctx.services.db_pool,
        ctx.services.credential_manager.as_ref(),
    )
    .await
    {
        Ok(device) => device,
        Err(cause) => {
            return finish_maintenance_with_error(
                switch_id,
                ctx,
                format!(
                    "Switch {} maintenance ({}): {}",
                    switch_id, operation_label, cause
                ),
            )
            .await;
        }
    };

    let request = rms::BatchSetPowerStateRequest {
        nodes: Some(rms::NodeSet {
            nodes: vec![device],
        }),
        operation: operation as i32,
    };

    match rms_client.batch_set_power_state(request).await {
        Ok(response) => {
            let batch = response.response.unwrap_or_default();
            let stats = batch.stats.unwrap_or_default();

            if batch.status == rms::ReturnCode::Success as i32 && stats.failed_nodes == 0 {
                tracing::info!(
                    switch_id = %switch_id,
                    rack_id = %rack_id,
                    operation = operation_label,
                    successful_nodes = stats.successful_nodes,
                    "RMS BatchSetPowerState succeeded; returning Switch to Ready"
                );
                let mut txn = ctx.services.db_pool.begin().await?;
                db_switch::clear_switch_maintenance_requested(&mut txn, *switch_id).await?;
                return Ok(StateHandlerOutcome::transition(success_state).with_txn(txn));
            }

            let node_error = batch
                .node_results
                .iter()
                .find(|result| {
                    result.status != rms::ReturnCode::Success as i32
                        || !result.error_message.is_empty()
                })
                .map(|result| {
                    if result.error_message.is_empty() {
                        format!("status={}", result.status)
                    } else {
                        result.error_message.clone()
                    }
                });
            let summary = if !batch.message.is_empty() {
                batch.message.clone()
            } else if let Some(error) = node_error.as_ref() {
                error.clone()
            } else {
                format!(
                    "batch status {}, failed_nodes {}",
                    batch.status, stats.failed_nodes,
                )
            };

            tracing::warn!(
                switch_id = %switch_id,
                rack_id = %rack_id,
                operation = operation_label,
                batch_status = batch.status,
                successful_nodes = stats.successful_nodes,
                failed_nodes = stats.failed_nodes,
                summary = %summary,
                "RMS BatchSetPowerState returned a non-success result",
            );
            let cause = format!(
                "Switch {} maintenance ({}): RMS BatchSetPowerState failed: {}",
                switch_id, operation_label, summary
            );
            finish_maintenance_with_error(switch_id, ctx, cause).await
        }
        Err(error) => {
            let error = rack_manager_error("batch_set_power_state", error);
            let cause = format!(
                "Switch {} maintenance ({}): RMS BatchSetPowerState failed: {}",
                switch_id, operation_label, error
            );
            tracing::warn!(
                switch_id = %switch_id,
                rack_id = %rack_id,
                operation = operation_label,
                error = %error,
                "RMS BatchSetPowerState transport error",
            );
            finish_maintenance_with_error(switch_id, ctx, cause).await
        }
    }
}

pub(super) async fn build_switch_node_info(
    switch_id: &SwitchId,
    state: &Switch,
    rack_id: String,
    db_pool: &PgPool,
    credential_manager: &dyn CredentialManager,
) -> Result<rms::NodeInfo, String> {
    let bmc_mac = state
        .bmc_mac_address
        .ok_or_else(|| format!("switch {} has no BMC MAC address recorded", switch_id))?;

    let rows = db_switch::find_switch_endpoints_by_ids(db_pool, &[*switch_id])
        .await
        .map_err(|error| format!("failed to look up switch endpoints: {}", error))?;

    let endpoint = rows
        .into_iter()
        .find(|row| row.switch_id == *switch_id)
        .ok_or_else(|| format!("no endpoint info found for switch {}", switch_id))?;

    let (Some(nvos_mac), Some(nvos_ip)) = (endpoint.nvos_mac, endpoint.nvos_ip) else {
        return Err(format!(
            "switch {} is missing NVOS MAC or IP required for RMS power control",
            switch_id
        ));
    };

    let bmc_credentials = lookup_bmc_credentials(credential_manager, bmc_mac).await?;
    let nvos_credentials = lookup_nvos_credentials(credential_manager, bmc_mac).await?;

    Ok(rms::NodeInfo {
        node_id: switch_id.to_string(),
        rack_id,
        r#type: Some(rms::NodeType::Switch as i32),
        bmc_endpoint: Some(rms::Endpoint {
            interface: Some(rms::NetworkInterface {
                ip_address: endpoint.bmc_ip.to_string(),
                mac_address: bmc_mac.to_string(),
            }),
            port: SWITCH_BMC_PORT,
            credentials: Some(bmc_credentials),
            dangerously_accept_invalid_certs: true,
        }),
        host_endpoint: Some(rms::Endpoint {
            interface: Some(rms::NetworkInterface {
                ip_address: nvos_ip.to_string(),
                mac_address: nvos_mac.to_string(),
            }),
            port: 0,
            credentials: Some(nvos_credentials),
            dangerously_accept_invalid_certs: false,
        }),
    })
}

async fn lookup_bmc_credentials(
    credential_manager: &dyn CredentialManager,
    bmc_mac: MacAddress,
) -> Result<rms::Credentials, String> {
    let bmc_key = CredentialKey::BmcCredentials {
        credential_type: BmcCredentialType::BmcRoot {
            bmc_mac_address: bmc_mac,
        },
    };
    let creds = match credential_manager.get_credentials(&bmc_key).await {
        Ok(Some(creds)) => Some(creds),
        Ok(None) => None,
        Err(error) => {
            return Err(format!(
                "failed to read BMC credentials for {}: {}",
                bmc_mac, error
            ));
        }
    };

    let creds = match creds {
        Some(creds) => creds,
        None => {
            let sitewide_key = CredentialKey::BmcCredentials {
                credential_type: BmcCredentialType::SiteWideRoot,
            };
            credential_manager
                .get_credentials(&sitewide_key)
                .await
                .map_err(|error| format!("failed to read site-wide BMC credentials: {}", error))?
                .ok_or_else(|| {
                    format!("no BMC credentials configured for {} or sitewide", bmc_mac)
                })?
        }
    };

    credentials_to_rms(creds)
}

async fn lookup_nvos_credentials(
    credential_manager: &dyn CredentialManager,
    bmc_mac: MacAddress,
) -> Result<rms::Credentials, String> {
    let key = CredentialKey::SwitchNvosAdmin {
        bmc_mac_address: bmc_mac,
    };
    let creds = credential_manager
        .get_credentials(&key)
        .await
        .map_err(|error| format!("failed to read NVOS credentials for {}: {}", bmc_mac, error))?
        .ok_or_else(|| format!("no NVOS admin credentials configured for {}", bmc_mac))?;

    credentials_to_rms(creds)
}

fn credentials_to_rms(creds: Credentials) -> Result<rms::Credentials, String> {
    let Credentials::UsernamePassword { username, password } = creds;
    Ok(rms::Credentials {
        auth: Some(rms::credentials::Auth::UserPass(rms::UsernamePassword {
            username,
            password,
        })),
    })
}

async fn finish_maintenance_with_error(
    switch_id: &SwitchId,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
    cause: String,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    let mut txn = ctx.services.db_pool.begin().await?;
    db_switch::clear_switch_maintenance_requested(&mut txn, *switch_id).await?;
    Ok(StateHandlerOutcome::transition(SwitchControllerState::Error { cause }).with_txn(txn))
}
