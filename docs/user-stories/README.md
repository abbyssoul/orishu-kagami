# User stories

This directory captures user-facing goals, workflows, and acceptance criteria for the product, split by the application each story is written against:

- [`orishu/`](./orishu/README.md) — `orishu`, the decentralized runtime for scientific simulation, used by cluster users and administrators.
- [`kagami/`](./kagami/README.md) — `kagami`, the native experiment authoring and visualization client, used by scientist-researchers to define and inspect simulations without operating the cluster directly.

Kagami is an authenticated `orishu` client; it does not join cluster membership or introduce a second remote compute server (see [ADR 0001](../adr/0001-product-oriented-monorepo.md)). Its stories are therefore kept separate from `orishu`'s cluster- and workload-lifecycle stories, and each subdirectory defines its own persona and glossary. Read the relevant subdirectory's `README.md` before its stories.

## Scope

These documents capture user-facing goals, workflows, and acceptance criteria. They are not protocol specs, API contracts, or low-level implementation design documents.

## Story evaluation guidance

When a story leaves user-visible behavior ambiguous, apply the principle of least surprise for the intended persona. Prefer behavior that matches the product's documented mental model and terminology, avoids hidden mutation, makes no-op and failure outcomes explicit, and uses safe defaults instead of surprising automation.

## Story template

Use this structure for new stories and major rewrites:

```md
### <verb-led story title>

As a <persona>, I want to <take an action> so that <outcome/value>.

**Given** <starting state>
**When** <action>
**Then** <expected result>

**Acceptance criteria:**
- <observable requirement>
- <observable requirement>
```

## Reading order

1. [orishu/README.md](./orishu/README.md) - persona, glossary, and story order for worker setup, cluster administration, and workload lifecycle.
2. [kagami/README.md](./kagami/README.md) - persona, glossary, and story order for experiment authoring, visualization, and external control through MCP.
