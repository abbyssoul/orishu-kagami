# Contributing to orishu

Thank you for contributing to `orishu`.

This project is an active research effort in distributed systems. Contributions
are welcome across code, tests, documentation, design work, and bug reports,
but the bar is intentionally high: behavior changes must be spec-driven,
thoroughly validated, and safe for a decentralized runtime.

If you are new to the codebase, start with:

- [User's README.md](../../README.md) for project intent and current status
- [Project Hight-Level Design](../design.md) for architecture and protocol direction
- [Developers GUIDELINES](../../GUIDELINES.md) for engineering expectations
- [Project Glossary](backlog/docs/doc-003%20-%20Glossary.md) for canonical terminology
- [Decision Registry](backlog/decisions/) for durable architectural decisions

## What helps most

Useful contributions include:

- bug reports with clear reproduction steps
- documentation improvements
- tests for existing behavior
- fuzz targets for parsers, protocol handling, and network-facing code
- focused bug fixes
- design/spec updates for planned behavioral changes

Changes that usually need discussion before implementation:

- protocol or API changes
- distributed runtime behavior changes
- large refactors
- new external dependencies
- anything that introduces a central coordinator or weakens deterministic
  behavior

If a change is more than a small bug fix or documentation cleanup, open an
issue or create a proposal in `backlog/proposals/` first so the design and scope can be reviewed before a large
implementation lands.

## Ground rules

Contributors are expected to work like maintainers, not ticket processors.
Raise concerns when a proposal increases risk, adds unnecessary complexity, or
conflicts with project goals.

Project expectations:

- Keep changes scoped and **reviewable**. At the end of the day changes are reviewed by people with limited time and attention span.
- Prefer spec-first development for behavioral changes. For bugs, it help to refer to user story to understand if its a gap (in which case we need to capture desired behavior as a user story) or its a bug in implementation where observed behavior differs from stated in the docs. If its a new feature - we must assess how it interacts with existing features.
- Update tests with every code change that affects behavior. Do not drop test coverage.
- Update documentation in the same change when behavior or operator workflow changes.
- Do not add dependencies casually; ask maintainers first.
- Do not rely on a trusted network boundary.

`orishu` is a distributed system operating in an adversarial environment.
Authenticated clients and known peers may still send malformed, incomplete, or
hostile messages. All network-facing code is expected to be fuzz-tested. The
baseline reliability standard is: "no single message should be able to bring a
node down or disrupt the cluster".

## AI-assisted contributions

AI-assisted contributions are welcome here. Parts of this project have been
developed with AI assistance, and we do not reject contributions merely because
an AI tool was involved.

That said, the rules **are** strict:

- Human accountability is absolute. The human author is 100% responsible for
  the content of the contribution, regardless of how much of it was drafted,
  reworked, or suggested by an AI tool.
- Quality matters more than volume. Low-quality AI-generated slop, bulk
  patches, and contributions that clearly were not reviewed by the submitter
  will be rejected. It is the PR author's job to ensure the contribution is
  correct, useful, and maintainable.
- Work in small steps. New contributors are expected to understand the code
  they are changing. Using AI as a substitute for understanding the codebase or
  as a way to brute-force a review is discouraged.
- Validation is required. AI output must be manually reviewed, tested, and
  verified. "The AI did it" is not a valid excuse for bugs, regressions,
  security issues, or licensing problems.

Disclosure policy:

- If AI did the heavy lifting for a meaningful part of the contribution,
  disclose that in the PR description or commit metadata. An `Assisted-by:`
  trailer is acceptable.
- Routine use of AI for grammar, spelling, formatting, or simple rephrasing
  does not need disclosure.

AI tools may assist authors and reviewers, but they are not the final arbiter
for accepting or rejecting contributions. Merge decisions remain a human
responsibility.

## Workspace overview

The repository is a Rust workspace with four main components:

- `orishu/`: shared core library
- `orishu-worker/`: worker node runtime daemon. This is the most important part of the distributed system. 
- `orishu-ctl/`: operator CLI. Basic CLI interface for system administrator to manage cluster.
- `orishu-monitor/`: TUI admin client for admins who love to stare at graphs in a terminal. 

Keep changes localized to the component you are actually modifying. 
If a change crosses crate boundaries, its worth mentioning why in the PR.

## Development setup

Minimum setup:

1. Install Rust with [rustup](https://rustup.rs/).
2. Install the nightly toolchain if you need to run formatting:
   `rustup toolchain install nightly`
3. Optionally install [Docker](https://docs.docker.com/engine/install/) or
   [Podman](https://podman.io/docs/installation) if you need to build container
   images.
4. Install [`cargo-fuzz`](https://rust-fuzz.github.io/book/cargo-fuzz/setup.html)
   if you are working on parsers, protocol handling, or other network-facing
   code.

## How to work on a change

1. Read the relevant design and guideline documents before editing code.
2. For behavior changes, update the design/spec first or in the same change.
3. Make the smallest change that solves the problem.
4. Add or update tests alongside the implementation.
5. Update documentation when user-visible, operator-visible, or protocol
   behavior changes.
6. Open a PR with enough context for a reviewer to evaluate safety and intent.

Refactors should avoid changing tests and implementation simultaneously unless
the behavioral change requires it. Flaky tests are not acceptable.

## Validation checklist

Run the checks relevant to your change before opening or updating a PR.

Formatting:

```sh
cargo +nightly fmt
```

Linting:

```sh
cargo clippy --tests --all-features
```

Tests:

```sh
cargo test --all-features
```

Docs:

```sh
cargo doc --all-features
```

Fuzzing:

```sh
cargo fuzz list
cargo fuzz run <target>
```

If your change touches message parsing, protocol decoding, admission logic, or
other network-exposed paths, fuzz coverage is expected. If no suitable fuzz
target exists yet, add one.

## Reporting bugs

Use GitHub issues for bugs, regressions, and documentation problems.

A good bug report includes:

- what you were trying to do
- what you expected to happen
- what happened instead
- the exact command or API call used
- relevant logs or error output
- Rust version, OS, and architecture
- cluster topology details when the problem is distributed or timing-sensitive

If the issue may expose a security vulnerability, do not open a public issue.
Follow [SECURITY.md](./SECURITY.md).

## Suggesting features

Feature requests are welcome, but they need to fit the project's direction.
`orishu` is not a generic job scheduler; it is a decentralized runtime for
shared simulation state. Requests are more likely to be accepted when they:

- align with decentralized control
- preserve deterministic state evolution
- explain protocol compatibility impact
- include operator impact and rollout considerations
- describe why the change belongs in `orishu` rather than in external tooling

For significant features, start with an issue that describes the problem,
motivation, and a rough design.

## Pull requests

All changes go through pull requests and peer review.

PR expectations:

- keep the scope focused
- explain the problem and the chosen approach
- call out protocol, compatibility, or rollout implications
- list the tests you added or updated
- include documentation updates when behavior changes
- use a Draft PR for work in progress

Pull requests are squash-merged. Write the PR title so it can serve as the
final commit message. Conventional Commit style is preferred, for example:
`feat(worker): reject malformed peer envelopes`.

External contributions may be reviewed by multiple maintainers before merge.

## Code review bar

Review in this repository is closer to an audit mindset than a style pass.
Reviewers will look for:

- correctness under partial failure
- deterministic behavior
- protocol safety and compatibility
- adequate tests
- operational diagnosability
- unnecessary dependencies or complexity

A change is not complete if the code works only on the happy path.

## Style notes

Follow [GUIDELINES.md](./GUIDELINES.md) for coding and testing standards.
Important highlights:

- keep code simple and readable
- document non-obvious safety assumptions
- use `unwrap` and `expect` only under the documented project rules
- prefer public documentation over tribal knowledge

## Licensing

By submitting a contribution, you agree that your work will be licensed under the repository's existing license terms.
DO NOT submit PRs if you can not comply with code licensing. It is PR submitter responsibility to ensure that they contribute the work for which they have legal rights. 
