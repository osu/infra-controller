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

use async_trait::async_trait;
use model::machine::ManagedHostStateSnapshot;

use super::DpaInterfaceStateHandler;
use crate::errors::DpaManagerResult;
use crate::metrics::DpaMonitorMetrics;
use crate::{DpaMonitor, HandlerResult};

pub struct AstraInterfaceHandler;

macro_rules! astra_todo {
    ($state:expr) => {{
        tracing::warn!(
            state = $state,
            "Astra DPA interface state handler not yet implemented"
        );
        Ok(HandlerResult {
            new_state: None,
            txn: None,
        })
    }};
}

#[async_trait]
impl DpaInterfaceStateHandler for AstraInterfaceHandler {
    async fn handle_provisioning(
        &self,
        _monitor: &mut DpaMonitor,
        _mh: &mut ManagedHostStateSnapshot,
        _idx: usize,
        _metrics: &mut DpaMonitorMetrics,
    ) -> DpaManagerResult<HandlerResult> {
        astra_todo!("provisioning")
    }

    async fn handle_ready(
        &self,
        _monitor: &mut DpaMonitor,
        _mh: &mut ManagedHostStateSnapshot,
        _idx: usize,
        _metrics: &mut DpaMonitorMetrics,
    ) -> DpaManagerResult<HandlerResult> {
        astra_todo!("ready")
    }

    async fn handle_unlocking(
        &self,
        _monitor: &mut DpaMonitor,
        _mh: &mut ManagedHostStateSnapshot,
        _idx: usize,
        _metrics: &mut DpaMonitorMetrics,
    ) -> DpaManagerResult<HandlerResult> {
        astra_todo!("unlocking")
    }

    async fn handle_apply_firmware(
        &self,
        _monitor: &mut DpaMonitor,
        _mh: &mut ManagedHostStateSnapshot,
        _idx: usize,
        _metrics: &mut DpaMonitorMetrics,
    ) -> DpaManagerResult<HandlerResult> {
        astra_todo!("apply_firmware")
    }

    async fn handle_apply_profile(
        &self,
        _monitor: &mut DpaMonitor,
        _mh: &mut ManagedHostStateSnapshot,
        _idx: usize,
        _metrics: &mut DpaMonitorMetrics,
    ) -> DpaManagerResult<HandlerResult> {
        astra_todo!("apply_profile")
    }

    async fn handle_locking(
        &self,
        _monitor: &mut DpaMonitor,
        _mh: &mut ManagedHostStateSnapshot,
        _idx: usize,
        _metrics: &mut DpaMonitorMetrics,
    ) -> DpaManagerResult<HandlerResult> {
        astra_todo!("locking")
    }

    async fn handle_assigned(
        &self,
        _monitor: &mut DpaMonitor,
        _mh: &mut ManagedHostStateSnapshot,
        _idx: usize,
        _metrics: &mut DpaMonitorMetrics,
    ) -> DpaManagerResult<HandlerResult> {
        astra_todo!("assigned")
    }
}
