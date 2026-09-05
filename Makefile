SHELL := /bin/sh

CARGO ?= cargo
DOCKER ?= podman
ARGS ?=

.DEFAULT_GOAL := build

.PHONY: build test test-docs test-formation check ci fmt fmt-check lint docs docs-check clean \
	run-kagami run-worker run-ctl run-monitor smoke-kagami coverage deb \
	install-ctl install-worker docker-worker docker-ctl docker-monitor

build:
	$(CARGO) build --locked --workspace

test:
	$(CARGO) test --locked --workspace --all-targets

test-docs:
	$(CARGO) test --locked --workspace --doc

# Unix process/QUIC journey; optional telemetry is not required.
test-formation:
	$(CARGO) build --locked -p orishu-worker -p orishuctl
	python3 scripts/test_formation_evidence.py
	python3 scripts/check-formation-cli.py

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

docker-worker:
	$(DOCKER) build --target orishu-worker -t orishu-worker:dev .

docker-ctl:
	$(DOCKER) build --target orishu-ctl -t orishuctl:dev .

docker-monitor:
	$(DOCKER) build --target orishu-monitor -t orishu-monitor:dev .

clean:
	$(CARGO) clean
