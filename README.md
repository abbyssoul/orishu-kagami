# Orishu Kagami

Orishu Kagami is a Rust monorepo for authoring, running, and inspecting
distributed scientific simulations.

- **Orishu** is the simulation engine and decentralized cluster runtime.
- **Kagami** is the native client for authoring simulated environments,
  controlling Orishu workloads, and visualizing live or recorded observations.

Field CAD was the original proof of concept for the authoring, simulation, and
visualization workflow. It is not a separate product in this repository. Proven
ideas and implementations from Field CAD are being rebuilt behind Orishu's
models and client protocol as Kagami develops.

## Status

This is an early research project. The Orishu command-line and worker crates are
present, and Kagami has a functional native application shell and GPU scene
renderer. Kagami can select an Orishu endpoint, but remote workload control and
observation streaming are not connected yet.

## Get started

Install the Rust toolchain through [rustup](https://rustup.rs/). The checked-in
toolchain file selects the supported compiler and installs `rustfmt` and
`clippy`.

On Debian or Ubuntu, Kagami's windowing stack may also require:

```sh
sudo apt-get install libwayland-dev libxkbcommon-dev libx11-dev \
  libxcursor-dev libxrandr-dev libxi-dev
```

Build and verify the complete repository from its root:

```sh
make build
make test
make check
```

### Orishu

Start a local worker:

```sh
make run-worker
```

In another terminal, inspect it with the operator CLI:

```sh
make run-ctl ARGS="--help"
```

The worker defaults to a per-user Unix socket. Pass `--host` or set
`ORISHU_HOST` when using a different endpoint.

### Kagami

Start the native client:

```sh
make run-kagami
```

Select an Orishu endpoint explicitly with:

```sh
make run-kagami ARGS="--host cluster.example.com:6680"
```

The selected endpoint is visible in the prototype UI. Network connection and
streaming remain implementation work. To exercise only the offscreen renderer,
without opening a window, run `make smoke-kagami`.

## Repository map

```text
apps/       deployable binaries
libs/       shared Rust modules
docs/       project-wide architecture and development documentation
etc/        deployment configuration for Orishu workers
scripts/    repository checks and automation helpers
```

See the [documentation index](docs/README.md), [architecture](docs/architecture.md),
and [contribution guide](CONTRIBUTING.md) before making structural or behavioural
changes.

## License

Licensed under the [Apache License 2.0](LICENSE).
