// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

package types

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestMapComponentStatus_Compute(t *testing.T) {
	cases := []struct {
		name      string
		raw       string
		wantPhase Phase
		wantOps   []OperationType
	}{
		// Steady state — no operations blocked.
		{"ready", "Ready", PhaseReady, nil},
		{"start_assignment_cycle", "StartAssignmentCycle", PhaseReady, nil},

		// Initializing buckets — top-level Display heads.
		{"created", "Created", PhaseInitializing, allComputeOps()},
		{"dpu_discovering", "DPUDiscovering/Unknown", PhaseInitializing, allComputeOps()},
		{"dpu_initializing", "DPUInitializing/Init", PhaseInitializing, allComputeOps()},
		{"host_initializing", "HostInitializing/Init", PhaseInitializing, allComputeOps()},
		{"measuring", "Measuring/Boot", PhaseInitializing, allComputeOps()},
		{"pre_assigned_measuring", "PreAssignedMeasuring/Idle", PhaseInitializing, allComputeOps()},
		{"post_assigned_measuring", "PostAssignedMeasuring/MeasuredBoot/Idle", PhaseInitializing, allComputeOps()},
		{"bom_validating", "BomValidating/Some", PhaseInitializing, allComputeOps()},

		// InUse buckets — tenant owns the host, or core is mid-reprovision.
		{"assigned_ready", "Assigned/Ready", PhaseInUse, allComputeOps()},
		{"assigned_provisioning", "Assigned/Provisioning", PhaseInUse, allComputeOps()},
		{"assigned_reprovision", "Assigned/Reprovision/Init", PhaseInUse, allComputeOps()},
		{"waiting_for_cleanup", "WaitingForCleanup/Init", PhaseInUse, allComputeOps()},
		{"dpu_reprovision", "Reprovisioning/Init", PhaseInUse, allComputeOps()},
		{"host_reprovision", "HostReprovisioning/Init", PhaseInUse, allComputeOps()},

		// Terminal.
		{"failed", "Failed/SomeCause", PhaseError, allComputeOps()},
		{"force_deletion", "ForceDeletion", PhaseDeleting, allComputeOps()},

		// Defaults.
		{"empty", "", PhaseUnknown, nil},
		// Validation Display has no fixed prefix; treat as Initializing.
		{"validation_pass_through", "DhcpReachable", PhaseInitializing, allComputeOps()},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := MapComponentStatus(ComponentTypeCompute, tc.raw)
			assert.Equal(t, tc.wantPhase, got.Phase, "phase")
			assert.Equal(t, tc.wantOps, got.BlockedOperations, "blocked ops")
		})
	}
}

func TestMapComponentStatus_Switch(t *testing.T) {
	cases := []struct {
		name      string
		raw       string
		wantPhase Phase
		wantOps   []OperationType
	}{
		{"ready", `{"state":"ready"}`, PhaseReady, nil},
		{"created", `{"state":"created"}`, PhaseInitializing, allNVSwitchOps()},
		{"initializing", `{"state":"initializing"}`, PhaseInitializing, allNVSwitchOps()},
		{"configuring", `{"state":"configuring"}`, PhaseInitializing, allNVSwitchOps()},
		{"validating", `{"state":"validating"}`, PhaseInitializing, allNVSwitchOps()},
		{"bomvalidating", `{"state":"bomvalidating"}`, PhaseInitializing, allNVSwitchOps()},
		{
			"reprovisioning_with_substate",
			`{"state":"reprovisioning","reprovisioning_state":"WaitingForRackFirmwareUpgrade"}`,
			PhaseInUse,
			allNVSwitchOps(),
		},
		{"error", `{"state":"error"}`, PhaseError, allNVSwitchOps()},
		{"deleting", `{"state":"deleting"}`, PhaseDeleting, allNVSwitchOps()},

		// Invalid / unknown — fail closed.
		{"empty", "", PhaseUnknown, nil},
		{"garbage", "not-json", PhaseUnknown, nil},
		{"unknown_tag", `{"state":"warpdrive"}`, PhaseUnknown, nil},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := MapComponentStatus(ComponentTypeNVSwitch, tc.raw)
			assert.Equal(t, tc.wantPhase, got.Phase, "phase")
			assert.Equal(t, tc.wantOps, got.BlockedOperations, "blocked ops")
		})
	}
}

func TestMapComponentStatus_PowerShelf(t *testing.T) {
	cases := []struct {
		name      string
		raw       string
		wantPhase Phase
		wantOps   []OperationType
	}{
		{"ready", `{"state":"ready"}`, PhaseReady, nil},
		{"initializing", `{"state":"initializing"}`, PhaseInitializing, allPowerShelfOps()},
		{"fetching_data", `{"state":"fetchingdata"}`, PhaseInitializing, allPowerShelfOps()},
		{"configuring", `{"state":"configuring"}`, PhaseInitializing, allPowerShelfOps()},
		{
			"maintenance_with_op",
			`{"state":"maintenance","maintenance":{"operation":"poweron"}}`,
			PhaseInUse,
			allPowerShelfOps(),
		},
		{"error", `{"state":"error"}`, PhaseError, allPowerShelfOps()},
		{"deleting", `{"state":"deleting"}`, PhaseDeleting, allPowerShelfOps()},

		{"empty", "", PhaseUnknown, nil},
		{"garbage", "{", PhaseUnknown, nil},
		{"unknown_tag", `{"state":"breakdancing"}`, PhaseUnknown, nil},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := MapComponentStatus(ComponentTypePowerShelf, tc.raw)
			assert.Equal(t, tc.wantPhase, got.Phase, "phase")
			assert.Equal(t, tc.wantOps, got.BlockedOperations, "blocked ops")
		})
	}
}

func TestMapComponentStatus_UnsupportedType(t *testing.T) {
	got := MapComponentStatus(ComponentTypeTORSwitch, `{"state":"ready"}`)
	assert.Equal(t, PhaseUnknown, got.Phase)
	assert.Empty(t, got.BlockedOperations)
}

func TestComponentStatus_IsReady(t *testing.T) {
	assert.True(t, ComponentStatus{Phase: PhaseReady}.IsReady())
	assert.False(t, ComponentStatus{Phase: PhaseInUse}.IsReady())
	assert.False(t, ComponentStatus{
		Phase:             PhaseReady,
		BlockedOperations: []OperationType{OperationTypePowerControl},
	}.IsReady())
}

func TestComponentStatus_Blocks(t *testing.T) {
	s := ComponentStatus{BlockedOperations: []OperationType{OperationTypeFirmwareControl}}
	assert.True(t, s.Blocks(OperationTypeFirmwareControl))
	assert.False(t, s.Blocks(OperationTypePowerControl))
}

func allComputeOps() []OperationType {
	return []OperationType{OperationTypePowerControl, OperationTypeFirmwareControl}
}

func allNVSwitchOps() []OperationType {
	return []OperationType{OperationTypePowerControl, OperationTypeFirmwareControl}
}

func allPowerShelfOps() []OperationType {
	return []OperationType{OperationTypePowerControl, OperationTypeFirmwareControl}
}
