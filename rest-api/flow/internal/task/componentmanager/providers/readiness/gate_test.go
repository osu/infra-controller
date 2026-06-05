// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

package readiness

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/stretchr/testify/require"

	"github.com/NVIDIA/infra-controller/rest-api/flow/pkg/types"
)

// fakeReader is an in-memory StatusReader for tests. It is goroutine-safe
// so a test can mutate state from another goroutine while the gate polls.
type fakeReader struct {
	mu       sync.Mutex
	statuses map[uuid.UUID]*types.ComponentStatus
	hosts    map[uuid.UUID][]uuid.UUID
	calls    atomic.Int32
}

func newFakeReader() *fakeReader {
	return &fakeReader{
		statuses: map[uuid.UUID]*types.ComponentStatus{},
		hosts:    map[uuid.UUID][]uuid.UUID{},
	}
}

func (f *fakeReader) setStatus(id uuid.UUID, s *types.ComponentStatus) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.statuses[id] = s
}

func (f *fakeReader) setHosts(rackID uuid.UUID, hosts []uuid.UUID) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.hosts[rackID] = hosts
}

func (f *fakeReader) GetStatusesByComponentIDs(_ context.Context, ids []uuid.UUID) (map[uuid.UUID]*types.ComponentStatus, error) {
	f.calls.Add(1)
	f.mu.Lock()
	defer f.mu.Unlock()
	out := make(map[uuid.UUID]*types.ComponentStatus, len(ids))
	for _, id := range ids {
		if s, ok := f.statuses[id]; ok {
			out[id] = s
		}
	}
	return out, nil
}

func (f *fakeReader) GetHostComponentIDsByRackIDs(_ context.Context, rackIDs []uuid.UUID) (map[uuid.UUID][]uuid.UUID, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	out := make(map[uuid.UUID][]uuid.UUID, len(rackIDs))
	for _, id := range rackIDs {
		if h, ok := f.hosts[id]; ok {
			out[id] = h
		}
	}
	return out, nil
}

func readyStatus() *types.ComponentStatus {
	return &types.ComponentStatus{Phase: types.PhaseReady}
}

func inUseStatus() *types.ComponentStatus {
	return &types.ComponentStatus{
		Phase:             types.PhaseInUse,
		Reason:            "Assigned/Provisioning",
		BlockedOperations: []types.OperationType{types.OperationTypePowerControl, types.OperationTypeFirmwareControl},
	}
}

func TestWaitForComponentsReady_EmptyInputShortCircuits(t *testing.T) {
	g := NewDBGate(newFakeReader(), time.Second, 10*time.Millisecond)
	require.NoError(t, g.WaitForComponentsReady(context.Background(), nil, types.OperationTypePowerControl))
	require.NoError(t, g.WaitForComponentsReady(context.Background(), []uuid.UUID{}, types.OperationTypePowerControl))
}

func TestWaitForComponentsReady_NilGateShortCircuits(t *testing.T) {
	var g *DBGate
	require.NoError(t, g.WaitForComponentsReady(context.Background(), []uuid.UUID{uuid.New()}, types.OperationTypePowerControl))
}

func TestWaitForComponentsReady_AllReady(t *testing.T) {
	r := newFakeReader()
	id1, id2 := uuid.New(), uuid.New()
	r.setStatus(id1, readyStatus())
	r.setStatus(id2, readyStatus())

	g := NewDBGate(r, time.Second, 10*time.Millisecond)
	require.NoError(t, g.WaitForComponentsReady(context.Background(), []uuid.UUID{id1, id2}, types.OperationTypePowerControl))
	require.Equal(t, int32(1), r.calls.Load(), "ready on first poll should not retry")
}

func TestWaitForComponentsReady_MissingStatusIsPermissive(t *testing.T) {
	r := newFakeReader()
	id1 := uuid.New()
	// Intentionally no status set for id1.

	g := NewDBGate(r, 50*time.Millisecond, 10*time.Millisecond)
	require.NoError(t, g.WaitForComponentsReady(context.Background(), []uuid.UUID{id1}, types.OperationTypePowerControl))
}

func TestWaitForComponentsReady_TimesOutWhileBlocking(t *testing.T) {
	r := newFakeReader()
	id1 := uuid.New()
	r.setStatus(id1, inUseStatus())

	g := NewDBGate(r, 50*time.Millisecond, 10*time.Millisecond)
	err := g.WaitForComponentsReady(context.Background(), []uuid.UUID{id1}, types.OperationTypePowerControl)
	require.Error(t, err)
	require.Contains(t, err.Error(), id1.String())
}

func TestWaitForComponentsReady_TransitionsFromBlockingToReady(t *testing.T) {
	r := newFakeReader()
	id1 := uuid.New()
	r.setStatus(id1, inUseStatus())

	g := NewDBGate(r, time.Second, 10*time.Millisecond)

	// Flip to ready after a short delay, mid-poll.
	go func() {
		time.Sleep(25 * time.Millisecond)
		r.setStatus(id1, readyStatus())
	}()

	require.NoError(t, g.WaitForComponentsReady(context.Background(), []uuid.UUID{id1}, types.OperationTypePowerControl))
	require.GreaterOrEqual(t, r.calls.Load(), int32(2), "should have polled more than once")
}

func TestWaitForComponentsReady_PartialBlocking(t *testing.T) {
	r := newFakeReader()
	idReady, idBlocked := uuid.New(), uuid.New()
	r.setStatus(idReady, readyStatus())
	r.setStatus(idBlocked, inUseStatus())

	g := NewDBGate(r, 30*time.Millisecond, 10*time.Millisecond)
	err := g.WaitForComponentsReady(context.Background(), []uuid.UUID{idReady, idBlocked}, types.OperationTypePowerControl)
	require.Error(t, err)
	require.Contains(t, err.Error(), idBlocked.String())
	require.NotContains(t, err.Error(), idReady.String())
}

func TestWaitForComponentsReady_OperationScopedBlock(t *testing.T) {
	r := newFakeReader()
	id1 := uuid.New()
	// Only firmware is blocked; power control should proceed.
	r.setStatus(id1, &types.ComponentStatus{
		Phase:             types.PhaseReady,
		BlockedOperations: []types.OperationType{types.OperationTypeFirmwareControl},
	})

	g := NewDBGate(r, time.Second, 10*time.Millisecond)
	require.NoError(t, g.WaitForComponentsReady(context.Background(), []uuid.UUID{id1}, types.OperationTypePowerControl))

	err := g.WaitForComponentsReady(context.Background(), []uuid.UUID{id1}, types.OperationTypeFirmwareControl)
	require.Error(t, err)
}

func TestWaitForComponentsReady_ContextCancellationStopsPolling(t *testing.T) {
	r := newFakeReader()
	id1 := uuid.New()
	r.setStatus(id1, inUseStatus())

	g := NewDBGate(r, 10*time.Second, 50*time.Millisecond)
	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(20 * time.Millisecond)
		cancel()
	}()
	err := g.WaitForComponentsReady(ctx, []uuid.UUID{id1}, types.OperationTypePowerControl)
	require.Error(t, err)
	require.True(t, errors.Is(err, context.Canceled))
}

func TestWaitForComponentsReady_DedupsAndIgnoresNilUUID(t *testing.T) {
	r := newFakeReader()
	id1 := uuid.New()
	r.setStatus(id1, readyStatus())

	g := NewDBGate(r, time.Second, 10*time.Millisecond)
	require.NoError(t, g.WaitForComponentsReady(
		context.Background(),
		[]uuid.UUID{uuid.Nil, id1, id1, uuid.Nil},
		types.OperationTypePowerControl,
	))
}

func TestWaitForRackHostsReady_NoHosts_Skips(t *testing.T) {
	r := newFakeReader()
	rackID := uuid.New()
	// Rack has no hosts (switch-only / empty).
	g := NewDBGate(r, time.Second, 10*time.Millisecond)
	require.NoError(t, g.WaitForRackHostsReady(context.Background(), []uuid.UUID{rackID}, types.OperationTypePowerControl))
	require.Equal(t, int32(0), r.calls.Load(), "no hosts => no status reads")
}

func TestWaitForRackHostsReady_ResolvesAndDelegates(t *testing.T) {
	r := newFakeReader()
	rackID := uuid.New()
	host1, host2 := uuid.New(), uuid.New()
	r.setHosts(rackID, []uuid.UUID{host1, host2})
	r.setStatus(host1, readyStatus())
	r.setStatus(host2, inUseStatus())

	g := NewDBGate(r, 30*time.Millisecond, 10*time.Millisecond)
	err := g.WaitForRackHostsReady(context.Background(), []uuid.UUID{rackID}, types.OperationTypePowerControl)
	require.Error(t, err)
	require.Contains(t, err.Error(), host2.String())
}
