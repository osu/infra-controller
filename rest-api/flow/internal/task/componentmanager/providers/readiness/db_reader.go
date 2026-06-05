// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

package readiness

import (
	"context"
	"fmt"

	"github.com/google/uuid"
	"github.com/uptrace/bun"

	"github.com/NVIDIA/infra-controller/rest-api/flow/pkg/common/devicetypes"
	"github.com/NVIDIA/infra-controller/rest-api/flow/pkg/types"
)

// DBReader is the production StatusReader. It reads model.Component rows
// via the provided bun.IDB and exposes only the fields the gate needs.
type DBReader struct {
	idb bun.IDB
}

// NewDBReader builds a StatusReader backed by the given bun.IDB.
func NewDBReader(idb bun.IDB) *DBReader {
	return &DBReader{idb: idb}
}

// GetStatusesByComponentIDs implements StatusReader.
func (r *DBReader) GetStatusesByComponentIDs(ctx context.Context, ids []uuid.UUID) (map[uuid.UUID]*types.ComponentStatus, error) {
	if len(ids) == 0 {
		return map[uuid.UUID]*types.ComponentStatus{}, nil
	}

	// Select only the columns we need rather than the full model row;
	// status is jsonb-decoded by bun directly into *types.ComponentStatus.
	type row struct {
		bun.BaseModel `bun:"table:component,alias:c"`
		ID            uuid.UUID              `bun:"id"`
		Status        *types.ComponentStatus `bun:"status"`
	}

	var rows []row
	err := r.idb.NewSelect().
		Model((*row)(nil)).
		Column("id", "status").
		Where("id IN (?)", bun.In(ids)).
		Scan(ctx, &rows)
	if err != nil {
		return nil, fmt.Errorf("select component statuses: %w", err)
	}

	out := make(map[uuid.UUID]*types.ComponentStatus, len(rows))
	for _, r := range rows {
		out[r.ID] = r.Status
	}
	return out, nil
}

// GetHostComponentIDsByRackIDs implements StatusReader.
func (r *DBReader) GetHostComponentIDsByRackIDs(ctx context.Context, rackIDs []uuid.UUID) (map[uuid.UUID][]uuid.UUID, error) {
	if len(rackIDs) == 0 {
		return map[uuid.UUID][]uuid.UUID{}, nil
	}

	type row struct {
		bun.BaseModel `bun:"table:component,alias:c"`
		ID            uuid.UUID `bun:"id"`
		RackID        uuid.UUID `bun:"rack_id"`
	}

	var rows []row
	err := r.idb.NewSelect().
		Model((*row)(nil)).
		Column("id", "rack_id").
		Where("rack_id IN (?)", bun.In(rackIDs)).
		Where("type = ?", devicetypes.ComponentTypeToString(devicetypes.ComponentTypeCompute)).
		Scan(ctx, &rows)
	if err != nil {
		return nil, fmt.Errorf("select host components by rack: %w", err)
	}

	out := make(map[uuid.UUID][]uuid.UUID, len(rackIDs))
	for _, r := range rows {
		out[r.RackID] = append(out[r.RackID], r.ID)
	}
	return out, nil
}

var _ StatusReader = (*DBReader)(nil)
