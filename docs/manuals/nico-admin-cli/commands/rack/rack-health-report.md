# `nico-admin-cli rack health-report`

_[Hardware commands](../../hardware.md) › [rack](./rack.md) › **health-report**_

## NAME

nico-admin-cli-rack-health-report - Manage health report sources

## SYNOPSIS

**nico-admin-cli rack health-report** \[**--extended**\]
\[**--sort-by**\] \[**-h**\|**--help**\] \<*subcommands*\>

## DESCRIPTION

Manage health report sources

## OPTIONS

**--extended**  
Extended result output.

This used by measured boot, where basic output contains just what you
probably care about, and "extended" output also dumps out all the
internal UUIDs that are used to associate instances.

**--sort-by** *\<SORT_BY\>* \[default: primary-id\]  
Sort output by specified field\

\
*Possible values:*

- primary-id: Sort by the primary id

- state: Sort by state

**-h**, **--help**  
Print help (see a summary with -h)

## Examples

```sh
nico-admin-cli rack health-report show rack-123
nico-admin-cli rack health-report add rack-123 --template internal-maintenance
nico-admin-cli rack health-report remove rack-123 internal-maintenance
nico-admin-cli rack health-report print-empty-template
```

## Subcommands

| Subcommand | Description |
|---|---|
| [`show`](./rack-health-report-show.md) | List health report sources for a rack |
| [`add`](./rack-health-report-add.md) | Insert a health report source for a rack |
| [`print-empty-template`](./rack-health-report-print-empty-template.md) | Print an empty health report template |
| [`remove`](./rack-health-report-remove.md) | Remove a health report source from a rack |

---

**See also:** [Hardware commands](../../hardware.md) · [CLI reference index](../../README.md)
