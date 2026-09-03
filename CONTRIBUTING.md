# Contributing

Contributions to Orishu Kagami are welcome across runtime code, native UI,
numerical work, tests, documentation, and design.

## Before changing code

Read the [project context](CONTEXT.md), [architecture](docs/architecture.md), and
relevant [architecture decisions](docs/adr/README.md). For a substantial or
expensive-to-reverse change, open an issue before implementation.

The repository is product-oriented:

- deployable binaries belong in `apps/`;
- reusable modules with demonstrated interfaces belong in `libs/`;
- project-wide documentation belongs in `docs/`; and
- former repository names are not architectural seams.

Field CAD is reference material for Kagami. Do not bulk-copy it or introduce a
second server, runtime, scene authority, or UI product. Follow the
[migration strategy](docs/migration.md) when reusing its work.

## Development workflow

1. Make the smallest coherent change.
2. Add tests through the interface production callers use.
3. Update public documentation and ADRs when behaviour or ownership changes.
4. Run `make check`, `make build`, and `make docs`.
5. Describe risk, validation, and unfinished manual verification in the pull
   request.

Changes to parsers, network payloads, workload admission, or streamed data must
include malformed and oversized inputs. No message from a client or known node
is trusted merely because it is authenticated.

## Pull requests

Keep pull requests focused and reviewable. Explain:

- the user or operator problem;
- why the chosen module owns the behaviour;
- compatibility, protocol, persistence, or rollout effects;
- automated and manual checks performed; and
- any numerical reference, convergence, or performance evidence.

Pull requests are expected to pass formatting, Clippy with warnings denied,
tests, documentation checks, dependency audit, and CodeQL analysis.

## AI-assisted work

AI assistance is allowed, but the human author remains responsible for every
change. Review generated code and tests, reject unnecessary dependencies and
bulk churn, and disclose substantial assistance in the pull request or commit
metadata, for example with an `Assisted-by:` trailer.
