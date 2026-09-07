SHELL := /bin/sh

CARGO ?= cargo
DOCKER ?= podman
ARGS ?=
PROMTOOL ?= promtool
PROMETHEUS ?= prometheus
OTELCOL ?= otelcol
WORKER_OTELCOL_TARGET_DIR ?= target/worker-otelcol

.PHONY: test-formation-release-guard
test-formation-release-guard:
	python3 scripts/test_formation_release.py
	python3 scripts/check-formation-release.py --cargo "$(CARGO)"

.DEFAULT_GOAL := build

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

# Official pinned Collector; local receipt/outage recipe, no download or service install.
.PHONY: test-worker-otelcol
test-worker-otelcol:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --target-dir "$(WORKER_OTELCOL_TARGET_DIR)"
	python3 scripts/test_worker_otelcol.py
	python3 scripts/check-worker-otelcol.py --otelcol "$(OTELCOL)" --worker "$(WORKER_OTELCOL_TARGET_DIR)/debug/orishu-worker" --ctl "$(WORKER_OTELCOL_TARGET_DIR)/debug/orishuctl"

# Local mTLS proxy/worker/Prometheus journey; requires pinned external tools.
.PHONY: test-worker-monitoring-proxy
test-worker-monitoring-proxy:
	$(CARGO) build --locked -p orishu-worker -p orishuctl --features orishu-worker/observability
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
