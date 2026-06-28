# `nico-admin-cli power-shelf health-report remove`

_[Hardware commands](../../hardware.md) › [power-shelf](./power-shelf.md) › [health-report](./power-shelf-health-report.md) › **remove**_

## NAME

nico-admin-cli-power-shelf-health-report-remove - Remove a health report
source from a power shelf

## SYNOPSIS

**nico-admin-cli power-shelf health-report remove** \[**--extended**\]
\[**--sort-by**\] \[**-h**\|**--help**\] \<*POWER_SHELF_ID*\>
\<*REPORT_SOURCE*\>

## DESCRIPTION

Remove a health report source from a power shelf

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

\<*POWER_SHELF_ID*\>  
\<*REPORT_SOURCE*\>

## Examples

```sh
nico-admin-cli power-shelf health-report remove ps100htjtiaehv1n5vh67tbmqq4eabcjdng40f7jupsadbedhruh6rag1l0 internal-maintenance
```

---

**See also:** [Hardware commands](../../hardware.md) · [CLI reference index](../../README.md)
