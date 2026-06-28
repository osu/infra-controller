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

// The intent of the tests.rs file is to test the integrity of the
// command, including things like basic structure parsing, enum
// translations, and any external input validators that are
// configured. Specific "categories" are:
//
// Command Structure - Baseline debug_assert() of the entire command.
// Argument Parsing  - Ensure required/optional arg combinations parse correctly.

use carbide_test_support::Outcome::*;
use carbide_test_support::scenarios;
use clap::{CommandFactory, Parser};

use super::health_report::args::Args as HealthReportCommand;
use super::*;

const TEST_RACK_ID: &str = "rack-123";

// verify_cmd_structure runs a baseline clap debug_assert()
// to do basic command configuration checking and validation,
// ensuring things like unique argument definitions, group
// configurations, argument references, etc. Things that would
// otherwise be missed until runtime.
#[test]
fn verify_cmd_structure() {
    Cmd::command().debug_assert();
}

/////////////////////////////////////////////////////////////////////////////
// Argument Parsing
//
// This section contains tests specific to argument parsing,
// including testing required arguments, as well as optional
// flag-specific checking.

// show parses with or without a rack identifier: bare `show` targets all
// racks (no rack), while a trailing identifier scopes it to that one rack.
#[test]
fn parse_show_routes_to_show_variant() {
    scenarios!(
        run = |argv| {
            Cmd::try_parse_from(argv.iter().copied())
                .map(|cmd| match cmd {
                    Cmd::Show(args) => args.rack.map(|r| r.to_string()),
                    _ => panic!("expected Show variant"),
                })
                .map_err(drop)
        };
        "no args targets all racks" {
            &["rack", "show"][..] => Yields(None),
        }

        "trailing identifier scopes to one rack" {
            &["rack", "show", "rack-123"][..] => Yields(Some("rack-123".to_string())),
        }
    );
}

// parse_list ensures list parses with no arguments.
#[test]
fn parse_list() {
    let cmd = Cmd::try_parse_from(["rack", "list"]).expect("should parse list");

    assert!(matches!(cmd, Cmd::List(_)));
}

// parse_delete ensures delete parses with identifier.
#[test]
fn parse_delete() {
    let cmd = Cmd::try_parse_from(["rack", "delete", "rack-123"]).expect("should parse delete");

    match cmd {
        Cmd::Delete(args) => {
            assert_eq!(args.identifier, "rack-123");
        }
        _ => panic!("expected Delete variant"),
    }
}

// parse_state_history ensures state-history parses with rack ID.
#[test]
fn parse_state_history() {
    let cmd = Cmd::try_parse_from(["rack", "state-history", "ipp6-b03-gb-nvl-124-mini2"])
        .expect("should parse state-history");

    match cmd {
        Cmd::StateHistory(args) => {
            assert_eq!(args.rack_id, "ipp6-b03-gb-nvl-124-mini2".parse().unwrap());
        }
        _ => panic!("expected StateHistory variant"),
    }
}

// parse_health_report_subcommands verifies all rack health-report leaves and
// the `hr` alias route to the expected command with their operator inputs.
#[test]
fn parse_health_report_subcommands() {
    scenarios!(
        run = |argv| {
            Cmd::try_parse_from(argv.iter().copied())
                .map(|cmd| match cmd {
                    Cmd::HealthReport(HealthReportCommand::Show(args)) => {
                        format!("show:{}", args.rack_id)
                    }
                    Cmd::HealthReport(HealthReportCommand::Add(args)) => {
                        let source = match (args.template, args.health_report) {
                            (Some(template), None) => format!("template:{template:?}"),
                            (None, Some(report)) => format!("json:{report}"),
                            _ => panic!("clap should require exactly one health report source"),
                        };
                        format!(
                            "add:{}:{source}:message={}:replace={}:print-only={}",
                            args.rack_id,
                            args.message.as_deref().unwrap_or("none"),
                            args.replace,
                            args.print_only
                        )
                    }
                    Cmd::HealthReport(HealthReportCommand::Remove(args)) => {
                        format!("remove:{}:{}", args.rack_id, args.report_source)
                    }
                    Cmd::HealthReport(HealthReportCommand::PrintEmptyTemplate(_)) => {
                        "print-empty-template".to_string()
                    }
                    other => panic!("unexpected command: {other:?}"),
                })
                .map_err(drop)
        };
        "show lists rack health reports" {
            &["rack", "health-report", "show", TEST_RACK_ID][..] => Yields("show:rack-123".to_string()),
        }

        "hr alias routes to show" {
            &["rack", "hr", "show", TEST_RACK_ID][..] => Yields("show:rack-123".to_string()),
        }

        "add accepts a template and action flags" {
            &[
                "rack",
                "health-report",
                "add",
                TEST_RACK_ID,
                "--template",
                "internal-maintenance",
                "--message",
                "Firmware upgrade in progress",
                "--replace",
                "--print-only",
            ][..] => Yields(
                "add:rack-123:template:InternalMaintenance:message=Firmware upgrade in progress:replace=true:print-only=true".to_string()
            ),
        }

        "add accepts a raw JSON report" {
            &[
                "rack",
                "health-report",
                "add",
                TEST_RACK_ID,
                "--health-report",
                r#"{"source":"smoke"}"#,
            ][..] => Yields(
                r#"add:rack-123:json:{"source":"smoke"}:message=none:replace=false:print-only=false"#.to_string()
            ),
        }

        "remove accepts a rack and report source" {
            &[
                "rack",
                "health-report",
                "remove",
                TEST_RACK_ID,
                "internal-maintenance",
            ][..] => Yields("remove:rack-123:internal-maintenance".to_string()),
        }

        "print-empty-template needs no arguments" {
            &["rack", "health-report", "print-empty-template"][..] => Yields("print-empty-template".to_string()),
        }
    );
}

// parse_profile_show ensures profile show parses with rack ID.
#[test]
fn parse_profile_show() {
    let cmd = Cmd::try_parse_from(["rack", "profile", "show", "rack-123"])
        .expect("should parse profile show");

    match cmd {
        Cmd::Profile(profile::Args::Show(args)) => {
            assert_eq!(args.rack_id, "rack-123".parse().unwrap());
        }
        _ => panic!("expected Profile(Show) variant"),
    }
}

// Every malformed invocation is rejected at parse time -- a delete or a
// profile-show left without its required rack identifier.
#[test]
fn invalid_invocations_are_rejected() {
    scenarios!(
        run = |argv| {
            Cmd::try_parse_from(argv.iter().copied())
                .map(|_| ())
                .map_err(drop)
        };
        "delete without an identifier" {
            &["rack", "delete"][..] => Fails,
        }

        "profile show without a rack_id" {
            &["rack", "profile", "show"][..] => Fails,
        }

        "state-history without a rack_id" {
            &["rack", "state-history"][..] => Fails,
        }

        "health-report show without a rack_id" {
            &["rack", "health-report", "show"][..] => Fails,
        }

        "health-report add without a report source" {
            &["rack", "health-report", "add", TEST_RACK_ID][..] => Fails,
        }

        "health-report add with both report sources" {
            &[
                "rack",
                "health-report",
                "add",
                TEST_RACK_ID,
                "--template",
                "degraded",
                "--health-report",
                r#"{"source":"smoke"}"#,
            ][..] => Fails,
        }

        "health-report add rejects message with raw JSON" {
            &[
                "rack",
                "health-report",
                "add",
                TEST_RACK_ID,
                "--health-report",
                r#"{"source":"smoke"}"#,
                "--message",
                "must not be ignored",
            ][..] => Fails,
        }

        "health-report remove without a report source" {
            &["rack", "health-report", "remove", TEST_RACK_ID][..] => Fails,
        }
    );
}
