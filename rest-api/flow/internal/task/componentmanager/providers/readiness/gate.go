// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

// Package readiness provides ReadinessGate, the Flow-side guard that holds
// mutating component operations (power, firmware, …) until every targeted
// component is in a phase that permits them. It reads the persisted
// ComponentStatus that inventorysync writes, so callers no longer poll Core
// directly for state-machine state.
//
// Semantics:
//   - empty input → no-op success
//   - missing / unknown status → log and treat as permissive (fail-open),
//     because conflating "no data" with "in use" would block every
//     operation on the first transient gRPC blip
//   - timeout returns the offending component IDs in the error
package readiness

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"time"

	"github.com/google/uuid"
	"github.com/rs/zerolog/log"

	"github.com/NVIDIA/infra-controller/rest-api/flow/pkg/types"
)

// Default polling parameters, chosen to err on the side of waiting. 30 min
// is long enough to cover a running tenant terminate cycle; 5 s keeps DB
// load low while still feeling responsive against in-memory status that
// inventorysync refreshes every cycle.
const (
	DefaultWaitTimeout  = 30 * time.Minute
	DefaultPollInterval = 5 * time.Second
)

// StatusReader is the narrow data dependency of the gate. A DB-backed
// implementation lives in this package; tests inject fakes.
type StatusReader interface {
	// GetStatusesByComponentIDs returns the persisted ComponentStatus for
	// each requested Flow component UUID. Components without a status row
	// (or without an entry at all) are simply absent from the result map.
	GetStatusesByComponentIDs(ctx context.Context, ids []uuid.UUID) (map[uuid.UUID]*types.ComponentStatus, error)

	// GetHostComponentIDsByRackIDs returns, for each rack, the Flow
	// component UUIDs of its host (compute) members. Other component types
	// are intentionally excluded — the rack-scoped readiness check is a
	// tenant-safety guard, and tenants only attach to hosts.
	GetHostComponentIDsByRackIDs(ctx context.Context, rackIDs []uuid.UUID) (map[uuid.UUID][]uuid.UUID, error)
}

// Gate is the abstraction call sites depend on.
type Gate interface {
	// WaitForComponentsReady blocks until none of the listed components
	// block op (per their persisted ComponentStatus), or the gate's
	// timeout elapses.
	WaitForComponentsReady(ctx context.Context, componentIDs []uuid.UUID, op types.OperationType) error

	// WaitForRackHostsReady is the rack-scoped form: resolves each rack
	// to its host components, then delegates to WaitForComponentsReady.
	WaitForRackHostsReady(ctx context.Context, rackIDs []uuid.UUID, op types.OperationType) error
}

// DBGate is the production Gate. The reader is typically a *dbReader
// constructed via NewDBReader.
type DBGate struct {
	reader       StatusReader
	timeout      time.Duration
	pollInterval time.Duration
}

// NewDBGate builds a gate. Zero or negative timeout / interval values fall
// back to package defaults so callers can opt in to overrides without
// repeating them.
func NewDBGate(reader StatusReader, timeout, pollInterval time.Duration) *DBGate {
	if timeout <= 0 {
		timeout = DefaultWaitTimeout
	}
	if pollInterval <= 0 {
		pollInterval = DefaultPollInterval
	}
	return &DBGate{
		reader:       reader,
		timeout:      timeout,
		pollInterval: pollInterval,
	}
}

// WaitForComponentsReady implements Gate.
func (g *DBGate) WaitForComponentsReady(ctx context.Context, componentIDs []uuid.UUID, op types.OperationType) error {
	if g == nil || g.reader == nil || len(componentIDs) == 0 {
		return nil
	}

	unique := dedupSortedUUIDs(componentIDs)

	deadline := time.Now().Add(g.timeout)
	attempt := 0
	for {
		attempt++
		blocking, err := g.findBlocking(ctx, unique, op)
		if err != nil {
			return fmt.Errorf("readiness check failed: %w", err)
		}
		if len(blocking) == 0 {
			if attempt > 1 {
				log.Info().
					Stringers("component_ids", uuidStringers(unique)).
					Str("operation", string(op)).
					Int("attempts", attempt).
					Msg("Components ready, proceeding with operation")
			}
			return nil
		}

		if !time.Now().Before(deadline) {
			return fmt.Errorf(
				"timed out after %s waiting for components to become ready for %s: %s",
				g.timeout, op, uuidsJoin(blocking),
			)
		}

		log.Info().
			Stringers("blocking_component_ids", uuidStringers(blocking)).
			Str("operation", string(op)).
			Dur("poll_interval", g.pollInterval).
			Time("deadline", deadline).
			Msg("Components still blocking operation, deferring")

		if err := sleep(ctx, g.pollInterval); err != nil {
			return err
		}
	}
}

// WaitForRackHostsReady implements Gate.
func (g *DBGate) WaitForRackHostsReady(ctx context.Context, rackIDs []uuid.UUID, op types.OperationType) error {
	if g == nil || g.reader == nil || len(rackIDs) == 0 {
		return nil
	}

	uniqueRacks := dedupSortedUUIDs(rackIDs)

	hostsByRack, err := g.reader.GetHostComponentIDsByRackIDs(ctx, uniqueRacks)
	if err != nil {
		return fmt.Errorf("list host components for racks: %w", err)
	}

	all := make([]uuid.UUID, 0)
	for _, rackID := range uniqueRacks {
		all = append(all, hostsByRack[rackID]...)
	}

	if len(all) == 0 {
		// Switch-only / empty racks: the safety check is vacuously
		// satisfied. Log so the absence stays visible.
		log.Info().
			Stringers("rack_ids", uuidStringers(uniqueRacks)).
			Str("operation", string(op)).
			Msg("Rack readiness check: no host components found, skipping wait")
		return nil
	}

	return g.WaitForComponentsReady(ctx, all, op)
}

// findBlocking returns the subset of componentIDs whose persisted status
// currently blocks op. Components with no status row (e.g. brand-new or
// inventory hasn't run yet) are logged once per iteration and treated as
// permissive — see the package doc comment.
func (g *DBGate) findBlocking(ctx context.Context, componentIDs []uuid.UUID, op types.OperationType) ([]uuid.UUID, error) {
	statuses, err := g.reader.GetStatusesByComponentIDs(ctx, componentIDs)
	if err != nil {
		return nil, err
	}

	var blocking []uuid.UUID
	var missing []uuid.UUID
	for _, id := range componentIDs {
		s, ok := statuses[id]
		if !ok || s == nil {
			missing = append(missing, id)
			continue
		}
		if s.Blocks(op) {
			blocking = append(blocking, id)
		}
	}

	if len(missing) > 0 {
		log.Warn().
			Stringers("missing_component_ids", uuidStringers(missing)).
			Str("operation", string(op)).
			Msg("Readiness check: no persisted status for some components, treating them as permissive")
	}
	return blocking, nil
}

func sleep(ctx context.Context, d time.Duration) error {
	if d <= 0 {
		return nil
	}
	t := time.NewTimer(d)
	defer t.Stop()
	select {
	case <-ctx.Done():
		return errors.Join(errors.New("aborted while waiting for components to become ready"), ctx.Err())
	case <-t.C:
		return nil
	}
}

func dedupSortedUUIDs(in []uuid.UUID) []uuid.UUID {
	if len(in) == 0 {
		return nil
	}
	seen := make(map[uuid.UUID]struct{}, len(in))
	out := make([]uuid.UUID, 0, len(in))
	for _, id := range in {
		if id == uuid.Nil {
			continue
		}
		if _, ok := seen[id]; ok {
			continue
		}
		seen[id] = struct{}{}
		out = append(out, id)
	}
	sort.Slice(out, func(i, j int) bool {
		return out[i].String() < out[j].String()
	})
	return out
}

// uuidStringers adapts []uuid.UUID to zerolog's Stringers helper.
func uuidStringers(in []uuid.UUID) []fmt.Stringer {
	out := make([]fmt.Stringer, len(in))
	for i := range in {
		out[i] = in[i]
	}
	return out
}

func uuidsJoin(in []uuid.UUID) string {
	if len(in) == 0 {
		return ""
	}
	s := in[0].String()
	for _, id := range in[1:] {
		s += ", " + id.String()
	}
	return s
}
