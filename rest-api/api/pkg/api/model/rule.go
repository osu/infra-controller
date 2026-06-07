// SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
// SPDX-License-Identifier: Apache-2.0

package model

import (
	"encoding/json"
	"fmt"
	"net/url"
	"strconv"
	"time"

	"github.com/NVIDIA/infra-controller/rest-api/api/pkg/api/pagination"
	flowv1 "github.com/NVIDIA/infra-controller/rest-api/workflow-schema/flow/protobuf/v1"
)

// Operation type strings exposed by the REST API. These mirror the
// protobuf OperationType enum (with "unknown" excluded from the public surface)
// so that the wire form is the same word a YAML rule file uses for
// operation_type. Keep names lowercase snake_case.
const (
	APIOperationTypePowerControl    = "power_control"
	APIOperationTypeFirmwareControl = "firmware_control"
)

// ProtoToAPIOperationTypeName maps Flow's protobuf OperationType enum to the
// string form used in API responses.
var ProtoToAPIOperationTypeName = map[flowv1.OperationType]string{
	flowv1.OperationType_OPERATION_TYPE_POWER_CONTROL:    APIOperationTypePowerControl,
	flowv1.OperationType_OPERATION_TYPE_FIRMWARE_CONTROL: APIOperationTypeFirmwareControl,
}

// apiToProtoOperationType is the reverse of ProtoToAPIOperationTypeName.
var apiToProtoOperationType = map[string]flowv1.OperationType{
	APIOperationTypePowerControl:    flowv1.OperationType_OPERATION_TYPE_POWER_CONTROL,
	APIOperationTypeFirmwareControl: flowv1.OperationType_OPERATION_TYPE_FIRMWARE_CONTROL,
}

// operationTypeFromAPI parses an API operation_type string. The empty string
// is treated as "unset" (caller decides whether that is valid). Returns an
// error for unknown values so we don't silently accept garbage.
func operationTypeFromAPI(s string) (flowv1.OperationType, error) {
	if s == "" {
		return flowv1.OperationType_OPERATION_TYPE_UNKNOWN, nil
	}
	v, ok := apiToProtoOperationType[s]
	if !ok {
		return flowv1.OperationType_OPERATION_TYPE_UNKNOWN,
			fmt.Errorf("invalid operationType %q (expected one of: %s, %s)",
				s, APIOperationTypePowerControl, APIOperationTypeFirmwareControl)
	}
	return v, nil
}

// APIOperationRule is the API response model for an Operation Rule.
// Top-level metadata uses camelCase; nested ruleDefinition uses snake_case to
// round-trip 1:1 with Flow's documented YAML/JSON schema so users converting
// existing YAML rule files only need to drop the same keys into the JSON body.
type APIOperationRule struct {
	ID             string            `json:"id"`
	Name           string            `json:"name"`
	Description    string            `json:"description,omitempty"`
	OperationType  string            `json:"operationType"`
	OperationCode  string            `json:"operationCode"`
	RuleDefinition APIRuleDefinition `json:"ruleDefinition"`
	IsDefault      bool              `json:"isDefault"`
	Created        time.Time         `json:"created"`
	Updated        time.Time         `json:"updated"`
}

// APIRuleDefinition is the executable body of a rule. The shape matches
// flow/internal/task/operationrules.RuleDefinition exactly so we can
// unmarshal Flow's RuleDefinitionJson straight into it.
type APIRuleDefinition struct {
	Version string            `json:"version"`
	Steps   []APISequenceStep `json:"steps,omitempty"`
}

// APISequenceStep mirrors operationrules.SequenceStep. Durations are kept as
// strings (Go duration syntax, e.g. "30s", "2m") so the round-trip with Flow
// preserves the exact form the user authored and Flow does the parsing.
type APISequenceStep struct {
	ComponentType string            `json:"component_type"`
	Stage         int               `json:"stage"`
	MaxParallel   int               `json:"max_parallel"`
	Timeout       string            `json:"timeout,omitempty"`
	Retry         *APIRetryPolicy   `json:"retry,omitempty"`
	PreOperation  []APIActionConfig `json:"pre_operation,omitempty"`
	MainOperation APIActionConfig   `json:"main_operation"`
	PostOperation []APIActionConfig `json:"post_operation,omitempty"`
	DelayAfter    string            `json:"delay_after,omitempty"`
}

// APIActionConfig mirrors operationrules.ActionConfig.
type APIActionConfig struct {
	Name         string         `json:"name"`
	Timeout      string         `json:"timeout,omitempty"`
	PollInterval string         `json:"poll_interval,omitempty"`
	Parameters   map[string]any `json:"parameters,omitempty"`
}

// APIRetryPolicy mirrors operationrules.RetryPolicy.
type APIRetryPolicy struct {
	MaxAttempts        int     `json:"max_attempts"`
	InitialInterval    string  `json:"initial_interval"`
	BackoffCoefficient float64 `json:"backoff_coefficient"`
	MaxInterval        string  `json:"max_interval,omitempty"`
}

// FromProto populates an APIOperationRule from a Flow protobuf OperationRule.
// Returns an error if ruleDefinitionJson cannot be unmarshaled into the API
// schema (this should never happen for rules that were written by Flow itself).
func (r *APIOperationRule) FromProto(pbRule *flowv1.OperationRule) error {
	if pbRule == nil {
		return nil
	}
	if pbRule.GetId() != nil {
		r.ID = pbRule.GetId().GetId()
	}
	r.Name = pbRule.GetName()
	r.Description = pbRule.GetDescription()
	r.OperationType = enumOr(ProtoToAPIOperationTypeName, pbRule.GetOperationType(), "")
	r.OperationCode = pbRule.GetOperationCode()
	r.IsDefault = pbRule.GetIsDefault()
	if ts := pbRule.GetCreatedAt(); ts != nil {
		r.Created = ts.AsTime().UTC()
	}
	if ts := pbRule.GetUpdatedAt(); ts != nil {
		r.Updated = ts.AsTime().UTC()
	}

	if raw := pbRule.GetRuleDefinitionJson(); raw != "" {
		if err := json.Unmarshal([]byte(raw), &r.RuleDefinition); err != nil {
			return fmt.Errorf("invalid ruleDefinition from Flow: %w", err)
		}
	}
	return nil
}

// NewAPIOperationRule constructs an APIOperationRule from a Flow proto rule.
func NewAPIOperationRule(pbRule *flowv1.OperationRule) (*APIOperationRule, error) {
	r := &APIOperationRule{}
	if err := r.FromProto(pbRule); err != nil {
		return nil, err
	}
	return r, nil
}

// ~~~~~ Create ~~~~~ //

// APICreateRuleRequest is the JSON body for POST /rule.
//
// IsDefault is intentionally absent: rules are created as non-default and
// promoted to default via a dedicated path (not exposed in this MVP). See the
// rule API design doc for the rationale (atomic swap requires Flow's
// SetRuleAsDefault RPC, which has different semantics than CRUD update).
type APICreateRuleRequest struct {
	SiteID         string            `json:"siteId"`
	Name           string            `json:"name"`
	Description    string            `json:"description,omitempty"`
	OperationType  string            `json:"operationType"`
	OperationCode  string            `json:"operationCode"`
	RuleDefinition APIRuleDefinition `json:"ruleDefinition"`
}

// Validate runs basic shape validation. Deep validation (operation code
// membership, rule definition semantics) lives in Flow and is surfaced via
// the workflow error path; doing it again here would force the API layer to
// track Flow's evolving allow-list.
func (r *APICreateRuleRequest) Validate() error {
	if r.SiteID == "" {
		return fmt.Errorf("siteId is required")
	}
	if r.Name == "" {
		return fmt.Errorf("name is required")
	}
	if r.OperationType == "" {
		return fmt.Errorf("operationType is required")
	}
	if _, err := operationTypeFromAPI(r.OperationType); err != nil {
		return err
	}
	if r.OperationCode == "" {
		return fmt.Errorf("operationCode is required")
	}
	return nil
}

// ToProto converts the request into the Flow CreateOperationRuleRequest.
// Returns an error if the rule definition cannot be marshaled (shouldn't
// happen for well-formed input).
func (r *APICreateRuleRequest) ToProto() (*flowv1.CreateOperationRuleRequest, error) {
	opType, err := operationTypeFromAPI(r.OperationType)
	if err != nil {
		return nil, err
	}
	rdJSON, err := json.Marshal(r.RuleDefinition)
	if err != nil {
		return nil, fmt.Errorf("failed to encode ruleDefinition: %w", err)
	}
	return &flowv1.CreateOperationRuleRequest{
		Name:               r.Name,
		Description:        r.Description,
		OperationType:      opType,
		OperationCode:      r.OperationCode,
		RuleDefinitionJson: string(rdJSON),
	}, nil
}

// ~~~~~ Update ~~~~~ //

// APIUpdateRuleRequest is the JSON body for PATCH /rule/{id}.
//
// All mutable fields are optional pointers so unset means "leave unchanged".
// operationType / operationCode are intentionally immutable after creation
// (mirroring Flow's UpdateRule constraint) — change them by creating a new
// rule and deleting the old one. is_default is also immutable here; see
// APICreateRuleRequest comment.
type APIUpdateRuleRequest struct {
	SiteID         string             `json:"siteId"`
	Name           *string            `json:"name,omitempty"`
	Description    *string            `json:"description,omitempty"`
	RuleDefinition *APIRuleDefinition `json:"ruleDefinition,omitempty"`
}

// Validate enforces that the request actually carries at least one field to
// update. siteId is always required as it routes to the right Flow.
func (r *APIUpdateRuleRequest) Validate() error {
	if r.SiteID == "" {
		return fmt.Errorf("siteId is required")
	}
	if r.Name == nil && r.Description == nil && r.RuleDefinition == nil {
		return fmt.Errorf("at least one of name, description, ruleDefinition must be provided")
	}
	if r.Name != nil && *r.Name == "" {
		return fmt.Errorf("name cannot be empty when provided")
	}
	return nil
}

// ToProto converts the update request into the Flow UpdateOperationRuleRequest.
// ruleID is the path parameter from the request URL.
func (r *APIUpdateRuleRequest) ToProto(ruleID string) (*flowv1.UpdateOperationRuleRequest, error) {
	req := &flowv1.UpdateOperationRuleRequest{
		RuleId:      &flowv1.UUID{Id: ruleID},
		Name:        r.Name,
		Description: r.Description,
	}
	if r.RuleDefinition != nil {
		rdJSON, err := json.Marshal(r.RuleDefinition)
		if err != nil {
			return nil, fmt.Errorf("failed to encode ruleDefinition: %w", err)
		}
		s := string(rdJSON)
		req.RuleDefinitionJson = &s
	}
	return req, nil
}

// ~~~~~ Get / Delete (siteId via query) ~~~~~ //

// APIGetRuleRequest captures query parameters for GET /rule/{id}.
type APIGetRuleRequest struct {
	SiteID string `query:"siteId"`
}

func (r *APIGetRuleRequest) Validate() error {
	if r.SiteID == "" {
		return fmt.Errorf("siteId query parameter is required")
	}
	return nil
}

// APIDeleteRuleRequest captures query parameters for DELETE /rule/{id}.
type APIDeleteRuleRequest struct {
	SiteID string `query:"siteId"`
}

func (r *APIDeleteRuleRequest) Validate() error {
	if r.SiteID == "" {
		return fmt.Errorf("siteId query parameter is required")
	}
	return nil
}

// ~~~~~ List ~~~~~ //

// APIListRulesRequest binds query parameters for GET /rule. Pagination is
// bound separately via pagination.PageRequest.
type APIListRulesRequest struct {
	SiteID        string `query:"siteId"`
	OperationType string `query:"operationType"`
}

func (r *APIListRulesRequest) Validate() error {
	if r.SiteID == "" {
		return fmt.Errorf("siteId query parameter is required")
	}
	if _, err := operationTypeFromAPI(r.OperationType); err != nil {
		return err
	}
	return nil
}

// ToProto converts the list filters into the Flow ListOperationRulesRequest.
// Returns an error if operationType is invalid.
func (r *APIListRulesRequest) ToProto(page pagination.PageRequest) (*flowv1.ListOperationRulesRequest, error) {
	req := &flowv1.ListOperationRulesRequest{}
	if r.OperationType != "" {
		opType, err := operationTypeFromAPI(r.OperationType)
		if err != nil {
			return nil, err
		}
		req.OperationType = &opType
	}
	if page.PageSize != nil && *page.PageSize > 0 {
		limit := int32(*page.PageSize)
		req.Limit = &limit
	}
	// Flow uses offset-based pagination. Translate (pageNumber, pageSize) into
	// offset; this matches how task list pagination flows through Flow.
	if page.PageNumber != nil && page.PageSize != nil && *page.PageNumber > 0 && *page.PageSize > 0 {
		offset := int32((*page.PageNumber - 1) * (*page.PageSize))
		req.Offset = &offset
	}
	return req, nil
}

// QueryValues returns query parameters that participate in deterministic
// workflow ID hashing, including pagination fields so concurrent requests for
// different filters/pages do not reuse the same workflow execution.
func (r *APIListRulesRequest) QueryValues(page pagination.PageRequest) url.Values {
	v := url.Values{}
	v.Set("siteId", r.SiteID)
	if r.OperationType != "" {
		v.Set("operationType", r.OperationType)
	}
	if page.PageNumber != nil && *page.PageNumber != 0 {
		v.Set("pageNumber", strconv.Itoa(*page.PageNumber))
	}
	if page.PageSize != nil && *page.PageSize != 0 {
		v.Set("pageSize", strconv.Itoa(*page.PageSize))
	}
	return v
}
