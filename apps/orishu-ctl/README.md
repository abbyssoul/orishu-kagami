# orishuctl

`orishuctl` is the scriptable operator client for an Orishu cluster. It manages
cluster membership, workloads, checkpoints, result artifacts, logs, and audit
information. Experiment authoring and scientific visualization belong in
Kagami.

From the repository root:

```sh
make build
make run-ctl ARGS="--help"
```

The client uses the local per-user worker socket by default. Select another
node with `--host` or `ORISHU_HOST`. Run `orishuctl <command> --help` for the
implemented command surface.

See the [project architecture](../../docs/architecture.md) and root
[README](../../README.md).
