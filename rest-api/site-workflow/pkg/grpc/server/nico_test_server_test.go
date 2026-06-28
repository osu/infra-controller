// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

package server

import (
	"context"
	"testing"

	"github.com/gogo/status"
	"google.golang.org/grpc/codes"

	cwssaws "github.com/NVIDIA/infra-controller/rest-api/workflow-schema/schema/site-agent/workflows/v1"
)

func TestInvokeInstancePower(t *testing.T) {
	const instanceID = "12345678-1234-5678-90ab-cdef01234567"

	tests := []struct {
		name          string
		request       *cwssaws.InstancePowerRequest
		wantCode      codes.Code
		wantMessage   string
		wantResultNil bool
	}{
		{
			name:          "nil request",
			wantCode:      codes.InvalidArgument,
			wantMessage:   "Invalid request argument",
			wantResultNil: true,
		},
		{
			name:          "missing instance ID",
			request:       &cwssaws.InstancePowerRequest{},
			wantCode:      codes.InvalidArgument,
			wantMessage:   "Invalid request argument",
			wantResultNil: true,
		},
		{
			name: "empty instance ID",
			request: &cwssaws.InstancePowerRequest{
				InstanceId: &cwssaws.InstanceId{},
			},
			wantCode:      codes.InvalidArgument,
			wantMessage:   "Invalid request argument",
			wantResultNil: true,
		},
		{
			name: "reset existing instance",
			request: &cwssaws.InstancePowerRequest{
				InstanceId: &cwssaws.InstanceId{Value: instanceID},
				Operation:  cwssaws.InstancePowerRequest_POWER_RESET,
			},
			wantCode: codes.OK,
		},
		{
			name: "invalid operation for existing instance",
			request: &cwssaws.InstancePowerRequest{
				InstanceId: &cwssaws.InstanceId{Value: instanceID},
				Operation:  cwssaws.InstancePowerRequest_Operation(1),
			},
			wantCode:    codes.InvalidArgument,
			wantMessage: "Invalid operation in request",
		},
		{
			name: "unknown instance",
			request: &cwssaws.InstancePowerRequest{
				InstanceId: &cwssaws.InstanceId{Value: "87654321-4321-8765-09ba-fedcba987654"},
				Operation:  cwssaws.InstancePowerRequest_POWER_RESET,
			},
			wantCode:      codes.NotFound,
			wantMessage:   `Instance with ID "87654321-4321-8765-09ba-fedcba987654" not found`,
			wantResultNil: true,
		},
	}

	server := &NICoServerImpl{
		ins: map[string]*cwssaws.Instance{
			instanceID: {Id: &cwssaws.InstanceId{Value: instanceID}},
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			result, err := server.InvokeInstancePower(context.Background(), test.request)
			if gotCode := status.Code(err); gotCode != test.wantCode {
				t.Fatalf("InvokeInstancePower() code = %v, want %v; error = %v", gotCode, test.wantCode, err)
			}
			if test.wantMessage != "" && status.Convert(err).Message() != test.wantMessage {
				t.Errorf("InvokeInstancePower() message = %q, want %q", status.Convert(err).Message(), test.wantMessage)
			}
			if gotResultNil := result == nil; gotResultNil != test.wantResultNil {
				t.Errorf("InvokeInstancePower() result nil = %t, want %t", gotResultNil, test.wantResultNil)
			}
		})
	}
}
