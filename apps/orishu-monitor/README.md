# orishu-monitor

`orishu-monitor` is the interactive terminal operator client for Orishu.

**Status: terminal shell only.** This build opens the full-screen application —
navigation, sections, help, and terminal-safe shutdown — but it does **not**
connect to an Orishu worker. It reports no cluster state, holds no credentials,
polls nothing, and changes nothing. Use [`orishuctl`](../orishu-ctl/README.md)
for implemented cluster administration.

Every panel says so on screen. No placeholder, default, or zero value is ever
presented as though a worker had reported it, and the empty states describe a
capability that is absent rather than a connection that failed.

## Running it

From the repository root:

```sh
make build
make run-monitor
```

`orishu-monitor` is a full-screen application and needs a terminal on both
standard input and standard output. Run with either redirected and it refuses
before changing any terminal state, so a piped invocation never has escape
sequences written into it. `--help` and `--version` do not need a terminal.

It takes no other options. Address, credential, timeout, and refresh options
are deliberately absent: they must be designed together with `orishuctl`'s when
the live-integration slice lands, not guessed at here.

## Sections

| Section | Shows |
| --- | --- |
| **Overview** | What this build is, and that worker integration is not part of it. |
| **Members** | The cluster's nodes — empty in this build, with the reason. |
| **Help** | An overlay listing every implemented key. |

## Keys

The **help overlay (`?`) is the authoritative list**. It and the input
dispatcher are both driven by one table in `src/keys.rs`, so in the running
application a key cannot be documented without working, or work without being
documented.

The table below is a hand-maintained copy of that source for readers who are
not at a terminal. It can fall behind; the overlay cannot. Keys are fixed —
user-configurable shortcuts are not part of this slice.

| Key | Action |
| --- | --- |
| `Tab` | next section |
| `Shift-Tab` | previous section |
| `1` | go to Overview |
| `2` | go to Members |
| `Up` / `k` | move up |
| `Down` / `j` | move down |
| `?` | toggle help |
| `Esc` | close help / back |
| `q` / `Ctrl-C` | quit |

While the help overlay is open it takes navigation for itself, so closing it
returns you to the section you left.

## Terminal behaviour

- Raw mode, the alternate screen, and cursor visibility are acquired through one
  owned session guard, which rolls back partial initialization and restores the
  terminal on normal exit, on any error after entry, and on a panic.
- The window is redrawn on resize. Below 56 x 14 the shell renders a compact
  explanation instead of the panels rather than producing an invalid layout.
- The event loop polls on a fixed cadence and drains a bounded number of queued
  events per wake-up, so it neither spins while idle nor accumulates work from
  an input burst.

## What comes next

The product destination is story-level administrative parity with `orishuctl`,
so an operator can choose scripted or interactive operation without changing
cluster semantics. Raw formation-admission secrets are the deliberate permanent
exception: the TUI does not reveal, copy, or print a join token or join
material, and operators use the CLI's private-file workflow for that material.

Live views arrive only after the projections they read have an accepted owner,
a bounded public type, an implemented worker route, and real serialized-path
tests. See the
[shell task](../../docs/tasks/implement-orishu-monitor-admin-tui.md) and the
[roadmap](../../docs/roadmap/README.md) (package P-MONITOR).

Kagami is the graphical experiment-authoring and scientific-visualization
client. `orishu-monitor` remains focused on cluster operations.

See the [project architecture](../../docs/architecture.md) and root
[README](../../README.md).
