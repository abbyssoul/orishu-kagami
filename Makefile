SHELL := /bin/sh

CARGO ?= cargo
FUZZ_TOOLCHAIN ?= nightly
FUZZ_SECONDS ?= 30
FUZZ_FLAGS ?= -timeout=10 -rss_limit_mb=2048 -max_len=1048581
DOCKER ?= podman
ARGS ?=
PROMTOOL ?= promtool
PROMETHEUS ?= prometheus
OTELCOL ?= otelcol
WORKER_OTELCOL_TARGET_DIR ?= target/worker-otelcol
WORKER_PROMETHEUS_TARGET_DIR ?= target/worker-prometheus
WORKER_SERVICE_TARGET_DIR ?= target/worker-service-enabled
WORKER_SERVICE_MINIMAL_TARGET_DIR ?= target/worker-service-minimal
FORMATION_TELEMETRY_OUTPUT ?=
FORMATION_TELEMETRY_ARGS ?=
PODMAN ?= podman
WORKER_CONTAINER_BASE ?= docker.io/library/debian@sha256:abc9cb88a5587630d7f915f47b23b0668fe250fbfc6457aa4d52b534c1bbf73f

.PHONY: test-formation-release-guard
test-formation-release-guard:
	python3 scripts/test_formation_release.py
	python3 scripts/check-formation-release.py --cargo "$(CARGO)"

.DEFAULT_GOAL := build

.PHONY: fuzz-smoke bench-formation
# Two passes per target. The first lets libFuzzer ramp input length; the second
# pins length at -max_len, because the ramp alone stays far below the one-MiB
# frame ceiling in a smoke-length campaign and never reaches the oversize paths.
fuzz-smoke:
	$(CARGO) run --locked --manifest-path fuzz/Cargo.toml --example seed_corpus
	@set -e; for target in membership_messages worker_frames worker_peer worker_catchup; do \
		$(CARGO) +$(FUZZ_TOOLCHAIN) fuzz run $$target -- -max_total_time=$(FUZZ_SECONDS) $(FUZZ_FLAGS); \
		$(CARGO) +$(FUZZ_TOOLCHAIN) fuzz run $$target -- -max_total_time=$(FUZZ_SECONDS) $(FUZZ_FLAGS) -len_control=0; \
	done

bench-formation:
	$(CARGO) bench --locked -p orishu-membership --bench membership -- $(ARGS)
	$(CARGO) bench --locked -p orishu-worker --bench formation -- $(ARGS)

.PHONY: build test test-docs test-formation test-formation-lost-ack test-formation-issuer-loss test-formation-source-loss test-formation-dead-assignment test-formation-removed-assignment test-formation-blocked-assignment test-formation-excluded-restart test-formation-peer-ejection test-formation-lost-departure test-formation-lost-leave-response check ci fmt fmt-check lint docs docs-check clean \
	run-kagami run-worker run-ctl run-monitor smoke-kagami coverage deb \
	install-ctl install-worker install-monitor install-kagami \
	test-release-scripts docker-worker docker-ctl docker-monitor

build:
	$(CARGO) build --locked --workspace

test:
	$(CARGO) test --locked --workspace --all-targets

test-docs:
	$(CARGO) test --locked --workspace --doc

# Explicit finite manual experiment; never part of timing-sensitive CI gates.
.PHONY: build-formation-telemetry measure-formation-telemetry
build-formation-telemetry:
	$(CARGO) build --locked --release -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --bins --example formation-telemetry-probe --target-dir target/formation-telemetry-enabled
	$(CARGO) build --locked --release -p orishu-worker -p orishuctl --target-dir target/formation-telemetry-omitted

measure-formation-telemetry:
	test -n "$(FORMATION_TELEMETRY_OUTPUT)"
	python3 scripts/test_formation_telemetry.py
	python3 scripts/measure-formation-telemetry.py --worker target/formation-telemetry-enabled/release/orishu-worker --omitted target/formation-telemetry-omitted/release/orishu-worker --ctl target/formation-telemetry-enabled/release/orishuctl --probe target/formation-telemetry-enabled/release/examples/formation-telemetry-probe --otelcol "$(OTELCOL)" --output "$(FORMATION_TELEMETRY_OUTPUT)" $(FORMATION_TELEMETRY_ARGS)
	python3 scripts/summarize-formation-telemetry.py "$(FORMATION_TELEMETRY_OUTPUT)"

# Real peer and client sockets over a deliberately misrouting two-namespace
# veth topology; each role has its own negative control. Requires root and
# openssl: veth and namespace creation need CAP_NET_ADMIN. The placement task
# also documents a user-namespace alternative. Never part of `make check` or CI.
# Leftovers from a crashed run: check-worker-network-placement.py --cleanup
.PHONY: test-worker-network-placement
test-worker-network-placement:
	$(CARGO) build --locked -p orishu-worker -p orishuctl
	python3 -m unittest discover -s scripts -p 'test_worker_network_placement*.py'
	sudo python3 scripts/check-worker-network-placement.py --worker target/debug/orishu-worker --ctl target/debug/orishuctl

# Requires pinned external test tools; downloads nothing and binds loopback only.
.PHONY: test-worker-prometheus
test-worker-prometheus:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability
	python3 scripts/check-worker-prometheus.py --promtool "$(PROMTOOL)" --prometheus "$(PROMETHEUS)"

.PHONY: test-worker-trace-prometheus
test-worker-trace-prometheus:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing
	python3 scripts/test_worker_trace_collector.py
	python3 scripts/check-worker-prometheus.py --trace-metrics --promtool "$(PROMTOOL)" --prometheus "$(PROMETHEUS)"

.PHONY: test-worker-formation-prometheus
test-worker-formation-prometheus:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing
	python3 scripts/check-worker-prometheus.py --formation-alerts --trace-metrics --promtool "$(PROMTOOL)" --prometheus "$(PROMETHEUS)"

# Logging is independently enabled; closed stdout must not fail worker health.
.PHONY: test-worker-log-prometheus
test-worker-log-prometheus:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir "$(WORKER_PROMETHEUS_TARGET_DIR)"
	python3 scripts/test_worker_prometheus_logs.py
	python3 scripts/test_worker_trace_collector.py
	python3 scripts/check-worker-prometheus.py --log-metrics --worker "$(WORKER_PROMETHEUS_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_PROMETHEUS_TARGET_DIR)/debug/orishuctl" --promtool "$(PROMTOOL)" --prometheus "$(PROMETHEUS)"
	python3 scripts/check-worker-prometheus.py --log-metrics --trace-metrics --formation-alerts --worker "$(WORKER_PROMETHEUS_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_PROMETHEUS_TARGET_DIR)/debug/orishuctl" --promtool "$(PROMTOOL)" --prometheus "$(PROMETHEUS)"
	python3 scripts/check-worker-prometheus.py --log-metrics --trace-metrics --closed-log-output --worker "$(WORKER_PROMETHEUS_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_PROMETHEUS_TARGET_DIR)/debug/orishuctl" --promtool "$(PROMTOOL)" --prometheus "$(PROMETHEUS)"

.PHONY: test-worker-dashboard
test-worker-dashboard:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing
	python3 scripts/check-worker-dashboard.py --promtool "$(PROMTOOL)" --prometheus "$(PROMETHEUS)"

# Runtime-only UUID-named user units; no package install or boot enablement.
.PHONY: test-worker-user-service
test-worker-user-service:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir "$(WORKER_SERVICE_TARGET_DIR)"
	$(CARGO) build --locked -p orishu-worker -p orishuctl --target-dir "$(WORKER_SERVICE_MINIMAL_TARGET_DIR)"
	python3 scripts/check-worker-user-service.py --worker "$(WORKER_SERVICE_TARGET_DIR)/debug/orishu-worker" --minimal-worker "$(WORKER_SERVICE_MINIMAL_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_SERVICE_MINIMAL_TARGET_DIR)/debug/orishuctl" --promtool "$(PROMTOOL)"

# Rootless local evaluation images; base must be pulled explicitly, apt runs in-image.
.PHONY: test-worker-container
test-worker-container:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir "$(WORKER_SERVICE_TARGET_DIR)"
	$(CARGO) build --locked -p orishu-worker -p orishuctl --target-dir "$(WORKER_SERVICE_MINIMAL_TARGET_DIR)"
	python3 scripts/check-worker-container.py --podman "$(PODMAN)" --base-image "$(WORKER_CONTAINER_BASE)" --worker "$(WORKER_SERVICE_TARGET_DIR)/debug/orishu-worker" --minimal-worker "$(WORKER_SERVICE_MINIMAL_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_SERVICE_MINIMAL_TARGET_DIR)/debug/orishuctl" --promtool "$(PROMTOOL)"

# Enabled stdout/OTLP collection in both existing source-built deployment examples.
.PHONY: test-worker-deployment-logs
test-worker-deployment-logs:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir "$(WORKER_SERVICE_TARGET_DIR)"
	python3 scripts/test_worker_deployment_receipts.py
	python3 scripts/test_worker_formation_receipts.py
	python3 scripts/test_worker_otelcol.py
	python3 scripts/check-worker-deployment-logs.py --worker "$(WORKER_SERVICE_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_SERVICE_TARGET_DIR)/debug/orishuctl" --otelcol "$(OTELCOL)" --podman "$(PODMAN)" --base-image "$(WORKER_CONTAINER_BASE)"

# Official pinned Collector; local receipt/outage recipe, no download or service install.
.PHONY: test-worker-otelcol
test-worker-otelcol:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir "$(WORKER_OTELCOL_TARGET_DIR)"
	python3 scripts/test_worker_otelcol.py
	python3 scripts/check-worker-otelcol.py --otelcol "$(OTELCOL)" --worker "$(WORKER_OTELCOL_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_OTELCOL_TARGET_DIR)/debug/orishuctl"

# Three-worker admission chains matched to bounded stdout captures.
.PHONY: test-worker-formation-otelcol
test-worker-formation-otelcol:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir "$(WORKER_OTELCOL_TARGET_DIR)"
	python3 scripts/test_worker_otelcol.py
	python3 scripts/test_worker_formation_receipts.py
	python3 scripts/check-worker-formation-otelcol.py --otelcol "$(OTELCOL)" --worker "$(WORKER_OTELCOL_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_OTELCOL_TARGET_DIR)/debug/orishuctl"

# Collector-only mTLS material; requires OpenSSL and uses no peer/operator keys.
.PHONY: test-worker-otelcol-mtls
test-worker-otelcol-mtls:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir "$(WORKER_OTELCOL_TARGET_DIR)"
	python3 scripts/test_worker_otelcol.py
	python3 scripts/check-worker-otelcol.py --mtls --otelcol "$(OTELCOL)" --worker "$(WORKER_OTELCOL_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_OTELCOL_TARGET_DIR)/debug/orishuctl"

# Local mTLS proxy/worker/Prometheus journey; requires pinned external tools.
.PHONY: test-worker-monitoring-proxy
test-worker-monitoring-proxy:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability
	python3 scripts/test_worker_monitoring_proxy.py
	python3 scripts/check-worker-monitoring-proxy.py --nginx "$(NGINX)" --worker target/debug/orishu-worker --ctl target/debug/orishuctl --promtool "$(PROMTOOL)" --prometheus "$(PROMETHEUS)"

# Unix process/QUIC journey with explicit per-worker diagnostic scrapes.
.PHONY: test-formation-observability
test-formation-observability:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability
	python3 scripts/check-formation-cli.py --observability

# Debug-only lost-ACK fault plus actual HTTP replay metrics; never a release profile.
.PHONY: test-formation-observability-lost-ack
test-formation-observability-lost-ack:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/formation-fault-test
	python3 scripts/check-formation-cli.py --observability --lost-join-ack

.PHONY: test-formation-observability-ejection
test-formation-observability-ejection:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/formation-fault-test
	python3 scripts/check-formation-cli.py --observability --peer-ejection

.PHONY: test-formation-observability-issuer-loss
test-formation-observability-issuer-loss:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/formation-fault-test
	python3 scripts/check-formation-cli.py --observability --issuer-loss

# Ordinary formation remains independent of optional telemetry.
test-formation:
	$(CARGO) build --locked -p orishu-worker -p orishuctl
	python3 scripts/test_formation_evidence.py
	python3 scripts/test_formation_http.py
	python3 scripts/check-formation-cli.py

# Harness-only slow authenticated client bodies; ordinary worker binaries.
.PHONY: test-formation-client-pressure
test-formation-client-pressure:
	$(CARGO) build --locked -p orishu-worker -p orishuctl
	python3 scripts/test_formation_http.py
	python3 scripts/test_formation_evidence.py
	python3 scripts/check-formation-cli.py --client-pressure

# Harness-only encrypted-UDP partition; ordinary worker binaries.
.PHONY: test-formation-policy-partition
test-formation-policy-partition:
	$(CARGO) build --locked -p orishu-worker -p orishuctl
	python3 scripts/test_formation_udp_relay.py
	python3 scripts/test_formation_http.py
	python3 scripts/test_formation_evidence.py
	python3 scripts/check-formation-cli.py --policy-partition

# Development-only fault build; never overwrite normal operator binaries.
test-formation-lost-ack:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --lost-join-ack

test-formation-issuer-loss:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --issuer-loss

.PHONY: test-formation-issuer-ejection
test-formation-issuer-ejection:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --issuer-ejection

test-formation-source-loss:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --source-loss

test-formation-dead-assignment:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --dead-assignment

test-formation-removed-assignment:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --removed-assignment

test-formation-blocked-assignment:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --blocked-assignment

test-formation-excluded-restart:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --excluded-restart

test-formation-peer-ejection:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --peer-ejection

test-formation-lost-departure:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test --target-dir target/formation-faults
	python3 scripts/check-formation-cli.py --worker target/formation-faults/debug/orishu-worker --ctl target/formation-faults/debug/orishuctl --lost-departure

test-formation-lost-leave-response:
	$(CARGO) build --locked -p orishu-worker -p orishuctl
	python3 scripts/test_formation_http.py
	python3 scripts/check-formation-cli.py --lost-leave-response

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

lint:
	$(CARGO) clippy --locked --workspace --all-targets -- -D warnings

docs:
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --locked --workspace --no-deps

docs-check:
	python3 scripts/check-docs.py

check: fmt-check lint test test-docs docs-check

ci: build check docs

run-kagami:
	$(CARGO) run --locked -p kagami -- $(ARGS)

run-worker:
	$(CARGO) run --locked -p orishu-worker -- $(ARGS)

run-ctl:
	$(CARGO) run --locked -p orishuctl -- $(ARGS)

run-monitor:
	$(CARGO) run --locked -p orishu-monitor -- $(ARGS)

smoke-kagami:
	$(CARGO) run --locked -p kagami-renderer --example smoke

coverage:
	$(CARGO) llvm-cov --locked --workspace --all-features --lcov --output-path lcov.info

deb:
	$(CARGO) deb --locked --strip -p orishuctl
	$(CARGO) deb --locked --strip -p orishu-worker
	$(CARGO) deb --locked --strip -p orishu-monitor

install-ctl:
	$(CARGO) install --locked --path apps/orishu-ctl

install-worker:
	$(CARGO) install --locked --path apps/orishu-worker

install-monitor:
	$(CARGO) install --locked --path apps/orishu-monitor

install-kagami:
	$(CARGO) install --locked --path apps/kagami

test-release-scripts:
	python3 -m unittest discover -s scripts/release -p 'test_*.py'

docker-worker:
	$(DOCKER) build --target orishu-worker -t orishu-worker:dev .

docker-ctl:
	$(DOCKER) build --target orishu-ctl -t orishuctl:dev .

docker-monitor:
	$(DOCKER) build --target orishu-monitor -t orishu-monitor:dev .

clean:
	$(CARGO) clean
