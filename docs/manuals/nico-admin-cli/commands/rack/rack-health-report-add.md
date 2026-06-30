# `nico-admin-cli rack health-report add`

_[Hardware commands](../../hardware.md) › [rack](./rack.md) › [health-report](./rack-health-report.md) › **add**_

## NAME

nico-admin-cli-rack-health-report-add - Insert a health report source
for a rack using exactly one of --health-report or --template

## SYNOPSIS

**nico-admin-cli rack health-report add** \[**--health-report**\]
\[**--template**\] \[**--message**\] \[**--replace**\]
\[**--print-only**\] \[**--extended**\] \[**--sort-by**\]
\[**-h**\|**--help**\] \<*RACK_ID*\>

## DESCRIPTION

Insert a health report source for a rack using exactly one of
--health-report or --template

## OPTIONS

**--health-report** *\<HEALTH_REPORT\>*  
New health report as JSON; mutually exclusive with --template

**--template** *\<TEMPLATE\>*  
Predefined template name; mutually exclusive with --health-report\

\
*Possible values:*

- host-update

- internal-maintenance

- out-for-repair

- degraded

- validation

- suppress-external-alerting

- mark-healthy

- stop-reboot-for-automatic-recovery-from-state-machine

- tenant-reported-issue

- request-online-repair

- request-repair

**--message** *\<MESSAGE\>*  
Message to fill in the template

**--replace**  
Replace all other health reports with this source

**--print-only**  
Print the report without sending it to the API

**--extended**  
Extended result output.

Used by measured boot. Basic output contains broadly-relevant information; extended output also dumps out all the internal UUIDs that are used to associate instances.

**--sort-by** *\<SORT_BY\>* \[default: primary-id\]  
Sort output by specified field

*Possible values:*

- primary-id: Sort by primary ID
- state: Sort by state

**-h**, **--help**  
Print help (see a summary with -h)

\<*RACK_ID*\>  
Rack whose health reports will be updated

## Examples

```sh
nico-admin-cli rack health-report add rack-123 --template internal-maintenance --message "Firmware upgrade in progress"
nico-admin-cli rack health-report add rack-123 --health-report '{"source":"admin-cli","observed_at":null,"successes":[],"alerts":[]}' --replace
nico-admin-cli rack health-report add rack-123 --template degraded --print-only
```

---

**See also:** [Hardware commands](../../hardware.md) · [CLI reference index](../../README.md)
