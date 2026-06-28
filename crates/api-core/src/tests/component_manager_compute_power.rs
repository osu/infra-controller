// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

use carbide_redfish::libredfish::test_support::RedfishSimAction;
use carbide_uuid::rack::{RackId, RackProfileId};
use component_manager::compute_tray_manager::Backend as ComputeBackend;
use component_manager::config::ComponentManagerConfig;
use component_manager::nv_switch_manager::Backend as NvSwitchBackend;
use component_manager::power_shelf_manager::Backend as PowerShelfBackend;
use db::rack as db_rack;
use librms::protos::rack_manager as rms;
use model::component_manager::PowerAction;
use model::rack::{MaintenanceActivity, RackConfig, RackState};
use model::test_support::ManagedHostConfig;
use rpc::common::{MachineIdList, SystemPowerControl};
use rpc::forge::ComponentPowerControlRequest;
use rpc::forge::component_power_control_request::Target;
use rpc::forge::forge_server::Forge;
use tonic::Request;

use crate::test_support::fixture_config::FixtureDefault as _;
use crate::tests::common::api_fixtures::site_explorer::new_host;
use crate::tests::common::api_fixtures::{
    TEST_RMS_RACK_PROFILE_ID, TestEnv, TestEnvOverrides, create_test_env_with_overrides,
};

fn rms_compute_overrides() -> TestEnvOverrides {
    TestEnvOverrides {
        component_manager_config: Some(ComponentManagerConfig {
            nv_switch_backend: NvSwitchBackend::Mock,
            power_shelf_backend: PowerShelfBackend::Mock,
            compute_tray_backend: ComputeBackend::Rms,
            ..Default::default()
        }),
        ..Default::default()
    }
}

async fn create_rms_compute_env(
    pool: sqlx::PgPool,
) -> Result<(TestEnv, RackId, model::machine::ManagedHostStateSnapshot), Box<dyn std::error::Error>>
{
    let env = create_test_env_with_overrides(pool.clone(), rms_compute_overrides()).await;

    let rack_id = RackId::new(uuid::Uuid::new_v4().to_string());
    let mut txn = pool.begin().await?;
    let rack = db_rack::create(
        txn.as_mut(),
        &rack_id,
        Some(&RackProfileId::new(TEST_RMS_RACK_PROFILE_ID)),
        &RackConfig::default(),
        None,
    )
    .await?;
    db_rack::try_update_controller_state(
        txn.as_mut(),
        &rack_id,
        rack.controller_state.version,
        rack.controller_state.version.increment(),
        &RackState::Ready,
    )
    .await?;
    txn.commit().await?;

    // Provision the host as a standalone machine first. Associating it with the RMS rack before
    // provisioning would make the background machine lifecycle consume the RMS mock while it is
    // still setting up the host, rather than leaving the mock isolated for this API request.
    let host = new_host(&env, ManagedHostConfig::default()).await?;

    let mut txn = pool.begin().await?;
    sqlx::query("UPDATE machines SET rack_id = $1 WHERE id = $2")
        .bind(rack_id.as_str())
        .bind(host.host_snapshot.id)
        .execute(txn.as_mut())
        .await?;
    txn.commit().await?;

    Ok((env, rack_id, host))
}

#[crate::sqlx_test]
async fn standalone_power_uses_core_under_global_rms_for_each_bypass_setting(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    struct Case {
        name: &'static str,
        bypass_state_controller: bool,
    }

    let env = create_test_env_with_overrides(pool, rms_compute_overrides()).await;
    let host = new_host(&env, ManagedHostConfig::default()).await?;

    for case in [
        Case {
            name: "normal dispatch",
            bypass_state_controller: false,
        },
        Case {
            name: "explicit state-controller bypass",
            bypass_state_controller: true,
        },
    ] {
        let timepoint = env.redfish_sim.timepoint();
        let response = env
            .api
            .component_power_control(Request::new(power_request(
                host.host_snapshot.id,
                case.bypass_state_controller,
            )))
            .await?
            .into_inner();

        assert_eq!(response.results.len(), 1, "{}", case.name);
        assert_eq!(
            response.results[0].component_id,
            host.host_snapshot.id.to_string(),
            "{}",
            case.name
        );
        assert_eq!(
            response.results[0].status,
            rpc::forge::ComponentManagerStatusCode::Success as i32,
            "{}",
            case.name
        );
        assert_eq!(
            env.redfish_sim.actions_since(&timepoint).all_hosts(),
            vec![RedfishSimAction::Power(
                libredfish::SystemPowerControl::ForceRestart
            )],
            "{}",
            case.name
        );
        assert!(
            env.rms_sim
                .submitted_batch_set_power_state_requests()
                .await
                .is_empty(),
            "{} must not dispatch to RMS",
            case.name
        );
    }

    let mut txn = env.pool.begin().await?;
    let racks = db_rack::find_by(
        txn.as_mut(),
        db::ObjectColumnFilter::<db_rack::IdColumn>::All,
    )
    .await?;
    assert!(
        racks
            .iter()
            .all(|rack| rack.config.maintenance_requested.is_none()),
        "standalone power must not queue rack maintenance"
    );

    Ok(())
}

fn power_request(
    machine_id: carbide_uuid::machine::MachineId,
    bypass_state_controller: bool,
) -> ComponentPowerControlRequest {
    ComponentPowerControlRequest {
        target: Some(Target::MachineIds(MachineIdList {
            machine_ids: vec![machine_id],
        })),
        action: SystemPowerControl::ForceRestart as i32,
        bypass_state_controller,
    }
}

#[crate::sqlx_test]
async fn rack_rms_power_queues_without_synchronous_backend_dispatch(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (env, rack_id, host) = create_rms_compute_env(pool).await?;

    let response = env
        .api
        .component_power_control(Request::new(power_request(host.host_snapshot.id, false)))
        .await?
        .into_inner();

    assert_eq!(response.results.len(), 1);
    assert_eq!(
        response.results[0].status,
        rpc::forge::ComponentManagerStatusCode::Success as i32
    );
    assert!(
        env.rms_sim
            .submitted_batch_set_power_state_requests()
            .await
            .is_empty(),
        "queued rack power must not dispatch RMS synchronously"
    );

    let mut txn = env.pool.begin().await?;
    let rack = db_rack::find_by(
        txn.as_mut(),
        db::ObjectColumnFilter::One(db_rack::IdColumn, &rack_id),
    )
    .await?
    .pop()
    .expect("queued rack");
    let scope = rack
        .config
        .maintenance_requested
        .expect("rack maintenance request");
    assert_eq!(scope.machine_ids, vec![host.host_snapshot.id]);
    assert_eq!(
        scope.activities,
        vec![MaintenanceActivity::PowerControl {
            action: PowerAction::ForceRestart,
        }]
    );

    Ok(())
}

#[crate::sqlx_test]
async fn rack_rms_power_bypass_dispatches_exact_action_directly(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let (env, rack_id, host) = create_rms_compute_env(pool).await?;
    env.rms_sim
        .queue_batch_set_power_state_response(Ok(rms::BatchSetPowerStateResponse {
            response: Some(rms::NodeBatchResponse {
                status: rms::ReturnCode::Success as i32,
                stats: Some(rms::NodeOperationStats {
                    total_nodes: 1,
                    successful_nodes: 1,
                    failed_nodes: 0,
                }),
                ..Default::default()
            }),
        }))
        .await;

    let response = env
        .api
        .component_power_control(Request::new(power_request(host.host_snapshot.id, true)))
        .await?
        .into_inner();

    assert_eq!(response.results.len(), 1);
    assert_eq!(
        response.results[0].status,
        rpc::forge::ComponentManagerStatusCode::Success as i32
    );
    let calls = env.rms_sim.submitted_batch_set_power_state_requests().await;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].operation, rms::PowerOperation::ForceRestart as i32);

    let mut txn = env.pool.begin().await?;
    let rack = db_rack::find_by(
        txn.as_mut(),
        db::ObjectColumnFilter::One(db_rack::IdColumn, &rack_id),
    )
    .await?
    .pop()
    .expect("bypassed rack");
    assert!(rack.config.maintenance_requested.is_none());

    Ok(())
}
