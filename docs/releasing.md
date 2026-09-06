# Building and publishing release artifacts

The release pipeline follows a build, verify, stage, then promote model. A dry
run and a published release use the same candidate jobs and bytes. This is
release-engineering scaffolding for pre-release software; an artifact does not
become a supported installation method until its persona journey and lifecycle
criteria in [P-INSTALL](tasks/publish-installable-artifacts.md) are accepted.

## Pull requests and main

The main CI workflow runs formatting, public-document links, Clippy, workspace
tests and documentation on Linux. It exercises Kagami's offscreen renderer,
validates the systemd unit, tests Kagami on Windows and macOS, and unit-tests
the deterministic packaging helpers. Workflow lint, CodeQL, and dependency
audits remain independent workflows.

## Candidate artifacts

[`release-artifacts.yml`](../.github/workflows/release-artifacts.yml) can be run
directly for a workspace version and ref. It builds:

- archives containing all four applications for Linux x86-64/ARM64 and macOS
  Apple Silicon/Intel;
- a Kagami archive for Windows x86-64; and
- separate `orishu-worker`, `orishuctl`, and preview `orishu-monitor` Debian
  packages for amd64 and arm64.

Each native binary is exercised before packaging. The workflow also installs
all four application packages with `cargo install --path` into a clean prefix.
This verifies the current workspace packages, not crates.io availability.
Archive construction is deterministic and unit-tested. Candidate artifacts are
retained for seven days.

Kagami is the only Windows artifact because the worker and operator tools do
not yet claim a tested Windows runtime contract. Kagami does not yet have a
native app bundle, signing, desktop integration, or installer; the zip is an
early executable archive.

## Release gate and publication

Run the `Release` workflow with the stable workspace version. Its default
`publish=false` mode:

1. pins the candidate to the dispatch commit and checks that it is on `main`;
2. runs the complete repository CI workflow against that exact commit;
3. invokes the candidate artifact workflow against the same commit;
4. requires the exact expected artifact set; and
5. creates a deterministic `SHA256SUMS` plus a downloadable candidate bundle.

Setting `publish=true` additionally requires approval of the GitHub `release`
environment. The workflow re-verifies the promoted bytes, creates or verifies
the immutable `v<version>` tag, publishes the GitHub release assets, and emits
GitHub build-provenance attestations over the checksum set. Configure branch
and environment protection before enabling publication in a public repository.

## Homebrew tap handoff

After publication, the release workflow can open a formula update PR rather
than writing directly to a tap's default branch. Configure:

- repository variable `HOMEBREW_TAP_REPOSITORY` as `OWNER/homebrew-TAP`; and
- release-environment secret `HOMEBREW_TAP_TOKEN` with permission to push a
  branch and open a pull request in that repository.

If the repository variable is absent, the Homebrew job is deliberately
skipped. The generated formula installs the four macOS command binaries from
the already-published, checksum-pinned archives. Kagami remains a command
binary at this stage, not a signed `.app` or cask.

## Cargo publication is a separate gate

`cargo install --path` works for all four applications and is tested in the
artifact workflow. `cargo install <package>` from crates.io is not implemented
yet. The application graph currently refers to unpublished workspace crates by
path without registry version requirements; for example, `cargo package -p
kagami` correctly rejects `kagami-renderer` for that reason.

Before adding a crates.io publish job, define the public crate set and publish
order, align package versions and metadata, add registry versions alongside
paths, run `cargo package`/`cargo publish --dry-run` for every crate, and verify
installation from the produced `.crate` files. A green checkout installation
must never be presented as proof of registry installation.

## Remaining product gates

The worker image still needs a secure first-run contract. Debian service-start
policy, clean install/upgrade/removal journeys, an SBOM and signing policy,
Kagami app packaging, and the fate of the preview monitor remain P-INSTALL
work. Official worker artifacts must eventually include accepted observability
capabilities while leaving listeners and exporters disabled by default; see
[ADR 0017](adr/0017-worker-operational-observability.md).

## Local checks

The deterministic packaging helpers use only the Python standard library:

```sh
make test-release-scripts
```

Release helpers derive the suite version from `[workspace.package]`; release
requests with a different or non-stable version fail before compilation.
