// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

package types

import (
	"encoding/json"
	"strings"
)

// ComponentStatus is Flow's view of a component's operability.
// It is derived from core's per-component state machine and recomputed
// on every inventory sync.
type ComponentStatus struct {
	Phase             Phase           `json:"phase"`
	Reason            string          `json:"reason,omitempty"`
	BlockedOperations []OperationType `json:"blocked_operations,omitempty"`
}

// IsReady returns true when the component is in Ready phase with no
// blocked operations of interest. It is a convenience for callers that
// only need a boolean go/no-go.
func (s ComponentStatus) IsReady() bool {
	return s.Phase == PhaseReady && len(s.BlockedOperations) == 0
}

// Blocks reports whether op is in BlockedOperations.
func (s ComponentStatus) Blocks(op OperationType) bool {
	for _, b := range s.BlockedOperations {
		if b == op {
			return true
		}
	}
	return false
}

// Equal reports whether two ComponentStatus values are identical. Needed
// because BlockedOperations is a slice and ComponentStatus is therefore
// not comparable with ==.
func (s ComponentStatus) Equal(other ComponentStatus) bool {
	if s.Phase != other.Phase || s.Reason != other.Reason {
		return false
	}
	if len(s.BlockedOperations) != len(other.BlockedOperations) {
		return false
	}
	for i := range s.BlockedOperations {
		if s.BlockedOperations[i] != other.BlockedOperations[i] {
			return false
		}
	}
	return true
}

// MapComponentStatus translates a raw core controller_state string into
// a ComponentStatus for the given component type. The raw form differs
// per type:
//   - Compute: ManagedHostState Display (e.g. "Ready", "Assigned/Provisioning").
//   - Switch / PowerShelf: JSON object with a "state" tag (e.g. {"state":"ready"}).
//
// Unrecognized inputs map to PhaseUnknown so callers fail closed.
func MapComponentStatus(componentType ComponentType, rawState string) ComponentStatus {
	switch componentType {
	case ComponentTypeCompute:
		return mapComputeStatus(rawState)
	case ComponentTypeNVSwitch:
		return mapSwitchStatus(rawState)
	case ComponentTypePowerShelf:
		return mapPowerShelfStatus(rawState)
	default:
		return ComponentStatus{Phase: PhaseUnknown, Reason: "unsupported component type: " + string(componentType)}
	}
}

func mapComputeStatus(raw string) ComponentStatus {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ComponentStatus{Phase: PhaseUnknown, Reason: "no controller_state from core"}
	}

	head := raw
	if i := strings.IndexByte(raw, '/'); i >= 0 {
		head = raw[:i]
	}

	switch head {
	case "Ready", "StartAssignmentCycle":
		return blockNoneIfReady(PhaseReady, "", ComponentTypeCompute)
	case "Created",
		"DPUDiscovering",
		"DPUInitializing",
		"HostInitializing",
		"Measuring",
		"PreAssignedMeasuring",
		"PostAssignedMeasuring",
		"BomValidating":
		return blockAll(PhaseInitializing, raw, ComponentTypeCompute)
	case "Assigned",
		"WaitingForCleanup",
		"Reprovisioning",
		"HostReprovisioning":
		return blockAll(PhaseInUse, raw, ComponentTypeCompute)
	case "Failed":
		return blockAll(PhaseError, raw, ComponentTypeCompute)
	case "ForceDeletion":
		return blockAll(PhaseDeleting, raw, ComponentTypeCompute)
	}

	// ManagedHostState::Validation Display delegates straight to its
	// inner ValidationState, so there is no "Validation/" prefix to key
	// on. Treat any unmatched value conservatively as Initializing —
	// safer than Unknown for compute since core is doing work.
	return blockAll(PhaseInitializing, raw, ComponentTypeCompute)
}

// switchStateEnvelope decodes the serde-tagged JSON emitted by core for
// SwitchControllerState / PowerShelfControllerState. Only the "state"
// discriminator is needed for the Phase decision; the full payload is
// kept in Reason for diagnostics.
type switchStateEnvelope struct {
	State string `json:"state"`
}

func mapSwitchStatus(raw string) ComponentStatus {
	tag, ok := decodeTaggedState(raw)
	if !ok {
		return ComponentStatus{Phase: PhaseUnknown, Reason: "undecodable switch state: " + raw}
	}
	switch tag {
	case "ready":
		return blockNoneIfReady(PhaseReady, "", ComponentTypeNVSwitch)
	case "created", "initializing", "configuring", "validating", "bomvalidating":
		return blockAll(PhaseInitializing, raw, ComponentTypeNVSwitch)
	case "reprovisioning":
		return blockAll(PhaseInUse, raw, ComponentTypeNVSwitch)
	case "error":
		return blockAll(PhaseError, raw, ComponentTypeNVSwitch)
	case "deleting":
		return blockAll(PhaseDeleting, raw, ComponentTypeNVSwitch)
	}
	return ComponentStatus{Phase: PhaseUnknown, Reason: "unknown switch state tag: " + tag}
}

func mapPowerShelfStatus(raw string) ComponentStatus {
	tag, ok := decodeTaggedState(raw)
	if !ok {
		return ComponentStatus{Phase: PhaseUnknown, Reason: "undecodable power shelf state: " + raw}
	}
	switch tag {
	case "ready":
		return blockNoneIfReady(PhaseReady, "", ComponentTypePowerShelf)
	case "initializing", "fetchingdata", "configuring":
		return blockAll(PhaseInitializing, raw, ComponentTypePowerShelf)
	case "maintenance":
		return blockAll(PhaseInUse, raw, ComponentTypePowerShelf)
	case "error":
		return blockAll(PhaseError, raw, ComponentTypePowerShelf)
	case "deleting":
		return blockAll(PhaseDeleting, raw, ComponentTypePowerShelf)
	}
	return ComponentStatus{Phase: PhaseUnknown, Reason: "unknown power shelf state tag: " + tag}
}

func decodeTaggedState(raw string) (string, bool) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return "", false
	}
	var env switchStateEnvelope
	if err := json.Unmarshal([]byte(raw), &env); err != nil || env.State == "" {
		return "", false
	}
	return env.State, true
}

// blockedOpsByType lists the operations Flow currently knows how to
// gate per component type. When Phase != Ready, all of these are
// blocked; Ready blocks none. Per-operation refinement (e.g. allowing
// power while a compute is in Assigned/Provisioning) is deferred.
var blockedOpsByType = map[ComponentType][]OperationType{
	ComponentTypeCompute:    {OperationTypePowerControl, OperationTypeFirmwareControl},
	ComponentTypeNVSwitch:   {OperationTypePowerControl, OperationTypeFirmwareControl},
	ComponentTypePowerShelf: {OperationTypePowerControl, OperationTypeFirmwareControl},
}

func blockAll(phase Phase, reason string, ct ComponentType) ComponentStatus {
	return ComponentStatus{
		Phase:             phase,
		Reason:            reason,
		BlockedOperations: append([]OperationType(nil), blockedOpsByType[ct]...),
	}
}

func blockNoneIfReady(phase Phase, reason string, _ ComponentType) ComponentStatus {
	return ComponentStatus{Phase: phase, Reason: reason}
}
