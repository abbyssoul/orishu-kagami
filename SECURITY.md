# Security policy

## Reporting a vulnerability

Do not open a public issue for a suspected vulnerability. Use the repository's
private GitHub security-advisory reporting flow and include:

- the affected binary, library, protocol, or file format;
- reproduction steps or a minimal proof of concept;
- expected impact and required privileges;
- affected versions or commits; and
- any proposed mitigation.

Maintainers will acknowledge the report, investigate it privately, and
coordinate disclosure after a fix is available. Avoid accessing data you do not
own or disrupting clusters while researching a report.

## Supported versions

Orishu Kagami is pre-release software. Security fixes are made on the latest
revision of the default branch. Older commits and development artifacts are not
supported release lines.

## Scope

Security-sensitive areas include Orishu's client and peer protocols, workload
packages, artifact parsing and storage, cluster admission and authentication,
and Kagami's handling of remote observations and authored files.

The project treats network peers and authenticated clients as untrusted. A
single malformed message must not crash a node, corrupt authoritative state, or
cause unbounded allocation or work.
