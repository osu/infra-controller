# `nico-admin-cli rack health-report show`

_[Hardware commands](../../hardware.md) › [rack](./rack.md) › [health-report](./rack-health-report.md) › **show**_

## NAME

nico-admin-cli-rack-health-report-show - List health report sources for
a rack

## SYNOPSIS

**nico-admin-cli rack health-report show** \[**--extended**\]
\[**--sort-by**\] \[**-h**\|**--help**\] \<*RACK_ID*\>

## DESCRIPTION

List health report sources for a rack

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
Rack ID to show health reports for

## Examples

```sh
nico-admin-cli rack health-report show rack-123
```

---

**See also:** [Hardware commands](../../hardware.md) · [CLI reference index](../../README.md)
