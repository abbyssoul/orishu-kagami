# Installing Orishu Kagami

Orishu Kagami is pre-release research software. Development builds from a
source checkout are available today; no binary artifact, native package,
container image, Cargo package, or desktop installer is currently published as
a supported release.

This page distinguishes commands that work in the repository from planned
distribution paths. Do not infer support from packaging scaffolding alone. The
[P-INSTALL task](tasks/publish-installable-artifacts.md) owns the work required
to turn the planned rows into tested user installation methods.

## Components

The product is delivered as separate programs for separate roles:

| Program | Role |
| --- | --- |
| `orishu-worker` | Self-contained worker runtime installed on each compute node. |
| `orishuctl` | Scriptable cluster operator client. |
| `orishu-monitor` | Interactive operator client; currently a placeholder and not ready for release. |
| `kagami` | Native experiment-authoring, submission, and visualization client. |

[ADR 0003](adr/0003-single-binary-distribution.md) requires each program to
start usefully without an external service or mandatory companion files. A
system package may provide optional configuration and service integration, but
it must not become the only usable form of a binary.

## Installation support matrix

| Method | Status | Intended audience |
| --- | --- | --- |
| Build and run from a checkout | Available for development | Contributors and evaluators |
| Local Cargo installation from a checkout | Available for `orishu-worker` and `orishuctl`; not a supported release | Developers evaluating command-line binaries |
| Locally built container targets | Build scaffolding exists; worker runtime defaults require correction and validation | Packaging developers |
| Linux release archive | Workflow scaffolding exists; no supported artifact is published | Future operators and researchers |
| Debian packages | `cargo-deb`, config, and systemd scaffolding exist; lifecycle and release integration remain unresolved | Future Linux operators |
| crates.io / `cargo install <package>` | Planned; packages are not published from this monorepo as a supported release | Operators and developers |
| Published OCI worker image | Planned; there is no supported registry image | Container operators |
| Homebrew | Planned for the installable release milestone | macOS operators and researchers |
| Snap | Not currently planned or implemented | — |
| Native Kagami installers | Planned; formats and supported platforms remain to be selected and tested | Researchers |

## Build and run from a source checkout

Install [rustup](https://rustup.rs/) and GNU Make. The checked-in toolchain file
selects Rust 1.97 and installs the required formatter and linter components.

```sh
git clone https://github.com/abbyssoul/orishu-kagami.git
cd orishu-kagami
make build
make test
make check
```

Linux systems building Kagami may also need the window-system development
packages listed in the root
[README](../README.md#build-and-run-from-source).

Run a local worker and inspect it in another terminal:

```sh
make run-worker
make run-ctl ARGS="cluster info"
```

The worker defaults to a private per-user Unix socket. Multiple workers require
different state directories and sockets. Read the
[worker guide](../apps/orishu-worker/README.md) and
[`orishuctl` guide](../apps/orishu-ctl/README.md) before enabling a peer or TCP
listener; mutation commands require the worker's operator credential even on a
local socket.

Run the Kagami development application with:

```sh
make run-kagami
```

Kagami's remote workload-control and observation path is not complete. Running
the application shell is not yet an end-to-end researcher installation.

## Local Cargo installation from a checkout

The repository can install the two implemented command-line binaries into the
active Cargo installation prefix:

```sh
make install-worker
make install-ctl
```

These commands build the current checkout with its lockfile. They do not prove
crates.io publication, upgrade compatibility, packaged service integration, or
support for that revision. Use them only for development evaluation.

## Packaging scaffolding available to developers

### Release archive

The release workflow can build a Linux x86-64 tar archive containing the four
binaries, README, and license. Manual workflow artifacts and tag-triggered
GitHub release upload are scaffolding, not an announced support promise. Before
publication the workflow must add integrity metadata, settle whether the
placeholder monitor belongs in the archive, test installation and first-run
journeys, and align supported versions and platforms.

### Containers

The root Dockerfile has separate development targets:

```sh
make docker-worker
make docker-ctl
make docker-monitor
```

Set `DOCKER=docker` when Docker is preferred over the Makefile's default
Podman command. These targets build local images only; the project publishes no
supported registry image yet.

The current worker image default selects a TCP listener without provisioning
the TLS certificate and key required by the worker. Consequently a successful
image build is not evidence of a usable default container run. P-INSTALL must
provide a secure, tested first-run contract before user-facing `docker run` or
Podman instructions are published.

### Debian packages and systemd

`cargo-deb` metadata exists for the three Orishu operational binaries, and the
repository carries a worker configuration and hardened systemd unit. Packaging
developers with `cargo-deb` installed can exercise the current scaffold with
`make deb`, but those packages are not release artifacts and their install
lifecycle is not yet accepted.

In particular, the worker package metadata currently requests automatic
service enablement and startup. Historical documentation instead said package
installation should leave first start explicit. P-INSTALL must choose and test
one policy before publication, considering safe defaults, configuration review,
credential creation, service failure behavior, upgrades, removal, and
non-interactive installs. Until then, neither behavior is documented as a user
guarantee.

## Planned release experience

The installable release milestone intends to provide:

- versioned release archives with checksums and provenance;
- a preferred native installation path for supported platforms;
- independently installable Cargo packages where appropriate;
- a non-root, securely configured worker container image;
- tested Debian package and systemd lifecycle behavior;
- a native Kagami installation and launch path;
- an explicit decision about shipping or withholding `orishu-monitor`;
- upgrade and compatibility guidance; and
- persona-based first-run verification for operators, researchers, and plugin
  developers.

Commands move from “planned” to “supported” only after CI installs the produced
artifact into a clean environment and performs its documented first-run and
upgrade checks. See the [implementation roadmap](roadmap/README.md#milestone-8--installable-release-candidate).

## Security notes

- Do not expose worker client or peer listeners without the documented TLS,
  identity, and authorization configuration.
- Do not bake private keys, operator credentials, or join material into images
  or release archives.
- A package installation must not silently weaken socket or state-directory
  permissions.
- Verify release integrity metadata once official artifacts exist.
- Pre-release development artifacts receive fixes only on the current default
  branch; see the [security policy](../SECURITY.md).
