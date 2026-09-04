# Orishu configuration model

Orishu binaries must start usefully without mandatory configuration files.
Operators may configure workers through files, environment variables, and
command-line arguments, with later/more specific sources taking precedence:

```text
configuration file < environment variable < command-line argument
```

This is an operator model, not permission for different sources to use
different schemas. Each option has one typed meaning and validation path.

Configuration and data locations follow XDG conventions where supported.
System packages may use platform locations such as `/etc/orishu` and
`/run/orishu`; user-scoped execution must not require writable system paths.
Credentials and private keys never belong in workload manifests, run
references, logs, or artifact identity.

## Worker configuration

Worker startup configuration includes listener addresses, TLS identity,
resource limits, peer/client/work admission flags, storage backend and
replication settings, and optional explicit introducer/join information.

Workers may intentionally expose only the client or peer listener appropriate
to their topology. A worker not accepting inbound peers may still join and
perform work; a worker not accepting clients remains observable through
explicitly designed relay/cached cluster views rather than by pretending it is
directly reachable.

In the MVP, `accepts.peers`, `accepts.clients`, `accepts.work`, and
`storage.replicas` are startup settings. Live changes are deferred in
`orishu-runtime-future-work.md`.

Environment-variable and CLI spelling must be documented beside the owning
binary and tested against the same typed configuration loader. Unknown,
malformed, contradictory, unsafe, or inaccessible configuration fails clearly;
there is no silent fallback that changes security or durability.

## Client configuration

Operator clients primarily configure endpoints, authentication, output, and
local persistence. Kagami additionally owns presentation and authoring
preferences. Endpoint hints never establish formation/run identity, and client
credentials never become shareable run-reference data.

Binary-specific supported options belong in each application's README. This
document defines the cross-application rules, including precedence and trust.
