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

//! Handler for SwitchControllerState::Ready.

use carbide_rack::rack_manager_error;
use carbide_uuid::switch::SwitchId;
use db::switch as db_switch;
use librms::protos::rack_manager as rms;
use model::switch::{ReProvisioningState, ReadyState, Switch, SwitchControllerState, SwitchStatus};
use sqlx::PgTransaction;
use state_controller::state_handler::{
    StateHandlerContext, StateHandlerError, StateHandlerOutcome,
};

use crate::context::SwitchStateHandlerContextObjects;
use crate::maintenance::build_switch_node_info;

/// Handles the Ready state for a switch.
///
/// If the switch is marked for deletion, transitions to `Deleting`.
/// If a maintenance request has been posted via `switch_maintenance_requested`,
/// transitions to `Maintenance` with the requested operation. If rack-level
/// reprovisioning has been requested, transitions to `ReProvisioning`.
/// Otherwise polls RMS for the current power state (best-effort observation)
/// and idles.
pub async fn handle_ready(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
) -> Result<StateHandlerOutcome<SwitchControllerState>, StateHandlerError> {
    if state.is_marked_as_deleted() {
        return Ok(StateHandlerOutcome::transition(
            SwitchControllerState::Deleting,
        ));
    }

    if let Some(req) = state.switch_maintenance_requested.as_ref() {
        tracing::info!(
            operation = ?req.operation,
            initiator = %req.initiator,
            "Switch maintenance requested; transitioning to Maintenance"
        );
        return Ok(StateHandlerOutcome::transition(
            SwitchControllerState::Maintenance {
                operation: req.operation,
            },
        ));
    }

    if let Some(req) = &state.switch_reprovisioning_requested {
        if req.initiator.starts_with("rack-") {
            tracing::info!(
                "Rack-level firmware upgrade requested — transitioning to WaitingForRackFirmwareUpgrade"
            );
            return Ok(StateHandlerOutcome::transition(
                SwitchControllerState::ReProvisioning {
                    reprovisioning_state: ReProvisioningState::WaitingForRackFirmwareUpgrade,
                },
            ));
        }

        tracing::warn!(
            "unknown initiator for switch reprovisioning request: {}",
            req.initiator
        );
        return Ok(StateHandlerOutcome::transition(
            SwitchControllerState::Error {
                cause: format!(
                    "unknown initiator for switch reprovisioning request: {}",
                    req.initiator
                ),
            },
        ));
    }

    let txn = poll_rms_power_state(switch_id, state, ctx).await;

    if let SwitchControllerState::Ready { ready_state } = &state.controller_state.value
        && let Some(status) = &state.status
    {
        let desired = ReadyState::from_power_state(&status.power_state);
        if *ready_state != desired {
            return Ok(StateHandlerOutcome::transition(
                SwitchControllerState::ready_from_power_state(&status.power_state),
            )
            .with_txn_opt(txn));
        }
    }

    Ok(StateHandlerOutcome::do_nothing().with_txn_opt(txn))
}

/// On a successful response, the observed `pstate` for this switch is
/// persisted to the `switches.status` column and the in-memory `state`
/// is updated to match.
async fn poll_rms_power_state(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
) -> Option<PgTransaction<'static>> {
    let Some(rms_client) = ctx.services.rms_client.as_ref() else {
        tracing::debug!(
            switch_id = %switch_id,
            "Switch Ready: skipping RMS BatchGetPowerState; RMS client not configured",
        );
        return None;
    };

    let Some(rack_id) = state.rack_id.as_ref() else {
        tracing::debug!(
            switch_id = %switch_id,
            "Switch Ready: skipping RMS BatchGetPowerState; switch has no rack association",
        );
        return None;
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
            tracing::debug!(
                switch_id = %switch_id,
                rack_id = %rack_id,
                cause = %cause,
                "Switch Ready: skipping RMS BatchGetPowerState; unable to build NodeSet",
            );
            return None;
        }
    };

    let request = rms::BatchGetPowerStateRequest {
        nodes: Some(rms::NodeSet {
            nodes: vec![device],
        }),
    };

    let rack_id_str = rack_id.to_string();
    let response = match rms_client.batch_get_power_state(request).await {
        Ok(response) => response,
        Err(error) => {
            let error = rack_manager_error("batch_get_power_state", error);
            tracing::warn!(
                switch_id = %switch_id,
                rack_id = %rack_id_str,
                error = %error,
                "RMS BatchGetPowerState transport error",
            );
            return None;
        }
    };

    let batch = response.response.clone().unwrap_or_default();
    let stats = batch.stats.unwrap_or_default();
    if !(batch.status == rms::ReturnCode::Success as i32 && stats.failed_nodes == 0) {
        tracing::warn!(
            switch_id = %switch_id,
            rack_id = %rack_id_str,
            batch_status = batch.status,
            successful_nodes = stats.successful_nodes,
            failed_nodes = stats.failed_nodes,
            message = %batch.message,
            "RMS BatchGetPowerState returned non-Success result",
        );
        return None;
    }

    tracing::info!(
        switch_id = %switch_id,
        rack_id = %rack_id_str,
        successful_nodes = stats.successful_nodes,
        pstates = ?response
            .node_power_states
            .iter()
            .map(|node| (node.node_id.as_str(), node.pstate.as_str()))
            .collect::<Vec<_>>(),
        "RMS BatchGetPowerState succeeded",
    );

    persist_observed_power_state(switch_id, state, ctx, &response.node_power_states).await
}

async fn persist_observed_power_state(
    switch_id: &SwitchId,
    state: &mut Switch,
    ctx: &mut StateHandlerContext<'_, SwitchStateHandlerContextObjects>,
    node_power_states: &[rms::NodePowerState],
) -> Option<PgTransaction<'static>> {
    let node_id = switch_id.to_string();
    let Some(observed) = node_power_states
        .iter()
        .find(|node| node.node_id == node_id)
    else {
        tracing::debug!(
            switch_id = %switch_id,
            "RMS BatchGetPowerState: no NodePowerState echoed for this switch; skipping status update",
        );
        return None;
    };

    let new_power_state = observed.pstate.to_lowercase();
    let new_status = match state.status.as_ref() {
        Some(existing) => SwitchStatus {
            switch_name: existing.switch_name.clone(),
            power_state: new_power_state.clone(),
            health_status: existing.health_status.clone(),
        },
        None => SwitchStatus {
            switch_name: state.config.name.clone(),
            power_state: new_power_state.clone(),
            health_status: String::new(),
        },
    };

    if state
        .status
        .as_ref()
        .is_some_and(|s| s.power_state == new_status.power_state)
    {
        tracing::debug!(
            switch_id = %switch_id,
            power_state = %new_status.power_state,
            "Switch status power_state unchanged; skipping DB write",
        );
        return None;
    }

    let previous_status = state.status.replace(new_status);

    let mut txn = match ctx.services.db_pool.begin().await {
        Ok(txn) => txn,
        Err(error) => {
            state.status = previous_status;
            tracing::warn!(
                switch_id = %switch_id,
                error = %error,
                "Switch Ready: failed to begin txn while persisting observed power state",
            );
            return None;
        }
    };

    if let Err(error) = db_switch::update(state, &mut txn).await {
        state.status = previous_status;
        tracing::warn!(
            switch_id = %switch_id,
            error = %error,
            "Switch Ready: failed to persist observed power state to DB",
        );
        return None;
    }

    tracing::info!(
        switch_id = %switch_id,
        power_state = %new_power_state,
        "Switch Ready: persisted observed power state from RMS",
    );

    Some(txn)
}
