# Review worker executor sizing after the formation PoC

Status: **planned, post-PoC performance review; no runtime policy selected**.
Owner: P-SCALE, consuming N-FORMATION and the physical experiment evidence.

## Outcome and current gap

Choose and document an evidence-backed executor policy for a single worker and
multiple workers sharing a machine, without changing membership correctness,
bounded IO or observability acceptance gates. Async IO does not imply a
single-threaded executor: the current worker uses default multi-thread Tokio.
The Pi diagnostic observed four executor threads plus the main thread per
worker, so four workers share a four-core Pi with sixteen executor threads.
The probe has a separate two-thread executor.

The desktop reports an i7-1370P with **14 physical cores and 20 logical CPUs**,
not twenty physical cores (`lscpu`, 2026-09-11). Its worker default was twenty
executor threads: thirty workers can create about 600 executor threads before
main threads, generators and other work. Created threads are not necessarily
runnable or busy threads. Affinity and cgroup CPU availability also matter.

Operator hypothesis: CPU competition, scheduler/context-switch costs and
migration/cache effects amplify the thirty-worker measurement noise. This is
**unverified**, not the established cause of the
[inconclusive thirty-worker baseline](../measurements/formation-post-diagnostic-2026-09-10.md).
The earlier [three-worker diagnostic](../measurements/formation-baseline-diagnostic-2026-09-10.md)
had thermal/frequency evidence for its throughput cliff, while a later bounded
noise diagnostic did not observe throttling. Do not conflate these episodes.
Allocation/locking and colocated generator costs remain competing explanations.

## Bounded slices

Additional input: the [route-corrected Pi HTTPS diagnostic](../measurements/formation-pi-ethernet-routing-2026-09-11.md)
reached 94.28k/sec with workers consuming 97–99% of each four-core host at
peak. Pi 4 delivery stayed around 10k/sec per host; doubling client concurrency
raised latency without increasing throughput. Profile this authenticated API
path and generator pressure before attributing cost to allocations, executor
thread count or any particular function. This does not establish the desktop
thirty-worker noise hypothesis or authorize a runtime policy change.

1. Freeze representative fixed-rate and saturation profiles, hardware topology,
   available CPUs, executor counts and generator placement. Separate local Unix
   and authenticated network measurements. Unix is a favourable transport, but
   colocated generators prevent treating its result as a rigorous worker ceiling.
2. Reproduce noise, then compare the current default with explicitly bounded
   one/two/four-thread multi-thread executors at identical offered load. A Tokio
   current-thread runtime is a separate candidate, not synonymous with a
   one-thread multi-thread runtime. Audit blocking tasks and shutdown/progress
   requirements before including it. Change one variable per comparison.
3. Measure worker and generator CPU separately, runnable pressure, per-thread
   context switches, scheduler wait, migration, frequency/temperature/throttling,
   latency, achieved/offered rate, memory and allocation rate. Follow the
   [hot-path guide](../high-performance-rust.md); stable RSS does not mean no
   allocations. Preserve every failed/noisy cell and instrumentation cost.
4. Recommend a policy only with repeatable evidence on single-worker hosts and
   colocated workers, including mixed Pi models. Verify formation, catch-up,
   readiness and shutdown under pressure. Record a new ADR if this changes an
   architectural execution boundary; a benchmark hypothesis alone is not an ADR.
5. If a runtime configuration is implemented, update the worker operator story,
   worker manual, deployment examples and experiment guide with defaults,
   oversubscription guidance, configuration bounds and CPU-budget interpretation.

## Acceptance and non-goals

- A reproducible comparison either identifies a useful policy or explicitly
  reports inconclusive evidence and its limiting factor; no forced winner.
- Retain existing M4 overhead/noise criteria. The new capacity target is
  **5,000 summary requests/second per worker**, configurable independently of
  the existing 500/second observability acceptance profile. It is neither
  simulation throughput nor peer-message throughput.
- No runtime thread changes, CPU affinity/governor changes, host tuning or new
  scientific performance claims are authorized by merely recording this task.
- This review is post-PoC work, not a newly invented formation completion gate.

Related: [physical experiment](implement-physical-formation-experiment.md),
[roadmap](../roadmap/README.md), [task index](README.md).
