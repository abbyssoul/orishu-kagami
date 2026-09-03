# Continuous integration and delivery

The repository keeps build, test, documentation, and release logic at the
monorepo root so every produced binary is evaluated together.

## Pull requests and main

The main CI workflow:

- runs formatting, public-document links, and Clippy with warnings denied;
- executes the root `make build` and `make test` contracts on Linux;
- builds doctests and Rust documentation with warnings denied;
- exercises Kagami's renderer offscreen with software rendering;
- validates the Orishu systemd unit; and
- builds and tests Kagami on Linux, Windows, and macOS.

Workflow files are linted independently. CodeQL runs for Rust on pushes, pull
requests, and a weekly schedule. `cargo audit` runs when the lockfile changes
and weekly so newly published advisories are detected without a code change.

## Release delivery

Pushing a `v*` tag runs the complete release gate, builds the workspace in
release mode, and publishes a Linux x86-64 bundle containing:

- `orishu-worker`;
- `orishuctl`;
- `orishu-monitor`;
- `kagami`;
- the root README and license.

The same workflow can be dispatched manually to produce an Actions artifact
without creating a GitHub release. Additional native installers and target
architectures should be added only after their runtime dependencies and signing
requirements are defined and tested.

The root Dockerfile produces separate images for the three Orishu operational
binaries. Kagami is a native desktop application and is not delivered as a
container.
