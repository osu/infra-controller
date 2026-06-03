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

//! Direct-invocation tests for the Switch `Maintenance` state handler.

use carbide_switch_controller::context::{
    SwitchStateHandlerContextObjects, SwitchStateHandlerServices,
};
use carbide_switch_controller::handler::SwitchStateHandler;
use carbide_switch_controller::metrics::SwitchMetrics;
use carbide_uuid::switch::SwitchId;
use db::switch as db_switch;
use model::switch::{Switch, SwitchControllerState, SwitchMaintenanceOperation};
use state_controller::db_write_batch::DbWriteBatch;
use state_controller::state_handler::{StateHandler, StateHandlerContext, StateHandlerOutcome};

use crate::tests::common::api_fixtures::site_explorer::new_switch;
use crate::tests::common::api_fixtures::{TestEnv, create_test_env};
use crate::tests::switch_state_controller::fixtures::switch::set_switch_controller_state;

async fn enter_maintenance(
    txn: &mut sqlx::PgConnection,
    switch_id: &SwitchId,
    operation: SwitchMaintenanceOperation,
) {
    db_switch::set_switch_maintenance_requested(txn, *switch_id, "test-initiator", operation)
        .await
        .unwrap();
    set_switch_controller_state(
        txn,
        switch_id,
        SwitchControllerState::Maintenance { operation },
    )
    .await
    .unwrap();
}

async fn load_switch(pool: &sqlx::PgPool, id: &SwitchId) -> Switch {
    let mut conn = pool.acquire().await.unwrap();
    db_switch::find_by_id(conn.as_mut(), id)
        .await
        .unwrap()
        .expect("switch should exist")
}

async fn run_handler(
    services: &mut SwitchStateHandlerServices,
    state: &mut Switch,
) -> StateHandlerOutcome<SwitchControllerState> {
    let handler = SwitchStateHandler::default();
    let mut metrics = SwitchMetrics::default();
    let mut writes = DbWriteBatch::default();
    let mut ctx = StateHandlerContext::<SwitchStateHandlerContextObjects> {
        services,
        metrics: &mut metrics,
        pending_db_writes: &mut writes,
    };
    let controller_state = state.controller_state.value.clone();
    let switch_id = state.id;
    handler
        .handle_object_state(&switch_id, state, &controller_state, &mut ctx)
        .await
        .expect("state handler should not return an error result")
}

async fn commit_and_extract_transition(
    mut outcome: StateHandlerOutcome<SwitchControllerState>,
) -> Option<SwitchControllerState> {
    if let Some(txn) = outcome.take_transaction() {
        txn.commit().await.unwrap();
    }
    match outcome {
        StateHandlerOutcome::Transition { next_state, .. } => Some(next_state),
        _ => None,
    }
}

fn services_without_rms(env: &TestEnv) -> SwitchStateHandlerServices {
    SwitchStateHandlerServices {
        db_pool: env.pool.clone(),
        rms_client: None,
        credential_manager: env.test_credential_manager.clone(),
    }
}

fn services_with_rms(env: &TestEnv) -> SwitchStateHandlerServices {
    SwitchStateHandlerServices {
        db_pool: env.pool.clone(),
        rms_client: env.rms_sim.as_rms_client(),
        credential_manager: env.test_credential_manager.clone(),
    }
}

#[crate::sqlx_test]
async fn maintenance_without_rms_transitions_to_error_and_clears_request(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    enter_maintenance(&mut txn, &switch_id, SwitchMaintenanceOperation::PowerOn).await;
    txn.commit().await?;

    let mut switch = load_switch(&pool, &switch_id).await;
    let mut services = services_without_rms(&env);
    let outcome = run_handler(&mut services, &mut switch).await;
    let next = commit_and_extract_transition(outcome).await;

    assert!(matches!(next, Some(SwitchControllerState::Error { .. })));

    let reloaded = load_switch(&pool, &switch_id).await;
    assert!(reloaded.switch_maintenance_requested.is_none());

    Ok(())
}

#[crate::sqlx_test]
async fn ready_transitions_to_maintenance_when_request_is_set(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = new_switch(&env, None, None).await?;

    let mut txn = pool.begin().await?;
    db_switch::set_switch_maintenance_requested(
        &mut txn,
        switch_id,
        "test-initiator",
        SwitchMaintenanceOperation::PowerOff,
    )
    .await?;
    set_switch_controller_state(&mut txn, &switch_id, SwitchControllerState::ready()).await?;
    txn.commit().await?;

    let mut switch = load_switch(&pool, &switch_id).await;
    let mut services = services_without_rms(&env);
    let outcome = run_handler(&mut services, &mut switch).await;

    assert!(matches!(
        outcome,
        StateHandlerOutcome::Transition {
            next_state: SwitchControllerState::Maintenance {
                operation: SwitchMaintenanceOperation::PowerOff,
            },
            ..
        }
    ));

    Ok(())
}

/// `Ready` should never invoke `batch_set_power_state`. It may attempt
/// `batch_get_power_state` for observation, but must not dispatch maintenance
/// power operations while idling in Ready.
#[crate::sqlx_test]
async fn ready_state_does_not_invoke_rms_set_power_state(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let env = create_test_env(pool.clone()).await;
    let switch_id = new_switch(&env, None, None).await?;

    {
        let mut txn = pool.acquire().await?;
        set_switch_controller_state(&mut txn, &switch_id, SwitchControllerState::ready()).await?;
    }

    let mut services = services_with_rms(&env);
    let mut switch = load_switch(&pool, &switch_id).await;
    let outcome = run_handler(&mut services, &mut switch).await;
    let _ = commit_and_extract_transition(outcome).await;

    let calls = env.rms_sim.submitted_batch_set_power_state_requests().await;
    assert!(
        calls.is_empty(),
        "Ready state must not call batch_set_power_state"
    );

    Ok(())
}
