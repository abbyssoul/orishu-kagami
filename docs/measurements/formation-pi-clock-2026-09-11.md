# Four-Pi read-only clock exchanges — 2026-09-11

Status: **clock-only SSH path verified; no load window or synchronization
acceptance**. Owner: [physical-host task](../tasks/implement-physical-formation-experiment.md).

The new `clock-check` action ran once against the supplied four-node inventory,
taking three nonce/run-bound exchanges per Pi. No worker, probe, lab-state
directory, package installation or clock-setting operation was started. Each
remote process accepted only clock requests (16-request cap, 20-second session
deadline) and exited 0 on EOF. The coordinator retained all twelve exchanges;
total elapsed time including SSH setup/cleanup was **2.426 seconds**.

Evidence: `/tmp/orishu-pi-clock.1kgSOq/check/clock-check.json`, run
`clock-80dd9e5245cd`. Private inventory and per-role receipts are retained beside
it, outside the repository. Source hashes recorded by the collection:

- Session bundle: `eb2e0ee3bac245e595478a82f60077831fe0e15f8d3679ffc210e23d93553010`
- Coordinator: `46534d66d2d34473a76fe5a0e14368d01a4c94124b7e6ae40c320bc0c9bb0cb5`

## Observations

Offsets are **Pi wall time minus coordinator wall time**, not accuracy against
an external time authority. The table gives the extent of the three separately
retained exchange intervals, not an averaged estimate or a proven constant
offset. Round-trip ranges include timestamp-read brackets and RPC overhead.

| Inventory role | Hardware | Round-trip extent (ms) | Offset interval extent (ms) |
| --- | --- | ---: | ---: |
| 0 | Pi 5, 8 GB | 3.130–3.370 | +48.702–+52.079 |
| 1 | Pi 5, 8 GB | 4.223–4.862 | +47.184–+52.452 |
| 2 | Pi 4, 8 GB | 5.539–11.310 | +47.590–+59.505 |
| 3 | Pi 4, 4 GB | 4.959–7.472 | +47.206–+54.677 |

The largest absolute endpoint of a coordinator wall-versus-monotonic change
interval was 2.686 microseconds. The causal offset envelope assumes no wall
step inside an individual exchange. Neither these short exchanges nor prior
NTP synchronization status bound future drift, hidden intermediate clock
steps, or overlap during load. No fastest sample was substituted for the full
set, and no host clock/service was changed.

## Consequences and validation

Pilot scheduling must account for the measured coordinator-to-node offset and
retain uncertainty. The approximately +50 ms common offset is not proof of a
large relative skew between Pis, nor evidence explaining worker slowdown.
Reviewed synchronization limits and before/after-window qualification remain
necessary. This is read-only calibration evidence, not a debit to a load-test
allowance, a throughput result, or a replacement for the 3/10/30 acceptance
curve. M4 remains open.

Five new clock-check tests cover the actual streamed bootstrap/session path,
read-only operation/configuration allowlists, command/nonce bounds, 3–5-node
collection and partial failure/EOF cleanup. All 119 focused Python tests,
formatting and documentation checks passed. No new Rust source/build change
was made for this increment; the full workspace Rust test suite was not rerun.

## Subsequent local start-proposal increment

An optional explicit-policy proposal path now computes per-role Unix starts
from all three exchanges, correcting offset direction and widening uncertainty
for freshness and assumed relative drift. It requires mutually consistent
intervals but uses their conservative union extent, includes a bounded wakeup
allowance, and rejects excessive conditional skew. It never dispatches or
claims that future clock behavior satisfies the assumptions.

Eight new planner tests and two additional clock-check tests pass, including
policy rejection without retry or loss of collected evidence. All 129 focused
Python tests, formatting and documentation checks passed. No Pi connection or
load run was made for this increment. The hardware hashes and results above
remain the earlier checkpoint; they do not qualify this later planner or a
newly selected policy. Operator limits, pilot allowance, fresh worker-session
integration and measured window overlap remain open.
