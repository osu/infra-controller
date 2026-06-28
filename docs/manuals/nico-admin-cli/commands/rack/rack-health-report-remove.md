# `nico-admin-cli rack health-report remove`

_[Hardware commands](../../hardware.md) › [rack](./rack.md) › [health-report](./rack-health-report.md) › **remove**_

## NAME

nico-admin-cli-rack-health-report-remove - Remove a health report source
from a rack

## SYNOPSIS

**nico-admin-cli rack health-report remove** \[**--extended**\]
\[**--sort-by**\] \[**-h**\|**--help**\] \<*RACK_ID*\>
\<*REPORT_SOURCE*\>

## DESCRIPTION

Remove a health report source from a rack

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

\<*RACK_ID*\>  
Rack whose health report source will be removed

\<*REPORT_SOURCE*\>  
Source name returned by \`health-report show\`

## Examples

```sh
nico-admin-cli rack health-report remove rack-123 internal-maintenance
```

---

**See also:** [Hardware commands](../../hardware.md) · [CLI reference index](../../README.md)
