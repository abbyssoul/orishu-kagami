# 0001 — Organize the monorepo around product roles

Status: **accepted**  
Date: **2026-09-03**

## Context

Orishu, Field CAD, and Kagami began as separate repositories. That history does
not describe the intended product. Orishu is the distributed simulation engine;
Kagami is its native authoring and visualization client; Field CAD is the proof
of concept from which selected behaviour may be rebuilt.

Preserving one workspace per former repository would retain duplicate models,
compute authorities, transports, user interfaces, and release processes. It
would also make integration an adapter between historical repositories instead
of a property of the product.

## Decision

Maintain one Cargo workspace organized by deployment and reusable role:

- `apps/` contains every produced binary, including `apps/kagami`.
- `libs/` contains shared modules with demonstrated interfaces.
- `docs/` describes the whole product.
- Field CAD does not exist as a product, application, workspace, or namespace in
  the new repository.

Reuse source selectively. Orishu's runtime/client implementation and Kagami's
Iced shell and renderer form the initial slice. Other Field CAD or Kagami code
must pass the admission test in `docs/migration.md` before migration.

Kagami is an authenticated Orishu client. It does not join cluster membership
or introduce a second remote compute server. Editable experiment intent remains
separate from the immutable workload accepted by Orishu.

## Consequences

- Root `make` commands and one lockfile define the build and test contract.
- Crate names describe current responsibilities, not repository provenance.
- The initial Kagami UI remains partly a shell while the experiment and
  observation interfaces are designed against Orishu.
- Useful Field CAD behaviour may be temporarily absent rather than copied behind
  the wrong interface.
- Releases and security policy cover every binary in the monorepo.
