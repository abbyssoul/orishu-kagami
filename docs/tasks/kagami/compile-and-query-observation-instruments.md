# Compile and query observation instruments

Status: **specified**; cluster query slices follow K8/K-RUN, with local-preview
parity following K-PREVIEW  
Work package: **K-OBSERVATION** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0004](../../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0011](../../adr/0011-classify-network-flows-and-baseline-observation-deltas.md)

## Outcome

K8's probes and sampling regions compile into bounded workload observation
requests, including exact trajectory retention. Local and Orishu runs publish
their readings through shared observation types, and the UI and MCP query the
same retained authority.

## Slices

1. Map stable instrument/channel identities into S-WORKLOAD requests with
   dimensions, cadence, retention and limits.
2. Emit readings at committed boundaries with workload/run, schema/model,
   precision, completeness and validity provenance.
3. Resolve object attachments from the authoritative boundary snapshot; never
   duplicate a moving object's pose into authored probe state.
4. Add bounded exact-boundary and time-range queries to Kagami's run projection
   and MCP adapter, including exact object trajectories and pagination/coverage
   semantics.
5. Test singular, outside-domain, unavailable, stale, truncated, reconnect and
   cross-run/baseline refusal paths.
6. Reuse the same contract from K-PREVIEW when it lands; local-preview parity is
   a later acceptance slice and does not block cluster probe queries in M3.

## Acceptance criteria

- An ideal probe or sampling region cannot affect a solve; a perturbing detector
  must be a modeled object component.
- UI and MCP return the same recorded values and provenance, including explicit
  undefined reasons rather than plausible zeroes.
- Query bounds are enforced before allocation/work, and continuations cannot
  cross run identity or baseline.
- Local preview and cluster execution satisfy the same observation contract.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Renderer styling, camera control, or MCP access to presentation state.
- Comparing runs or exporting analysis tables.
