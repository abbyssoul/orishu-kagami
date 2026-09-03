DOCKER ?= podman

.PHONY: build clean docs test-docs test deb

all: build

build:
	@cargo build

test:
	@cargo test

docs: 
	@cargo doc

test-docs:
	@cargo test --doc

.PHONY: coverage-summary
coverage-summary:
	@cargo llvm-cov -q --frozen --lcov --summary-only --output-path ./lcov-summary.info
	@cat ./lcov-summary.info | ./lcov_coverage.py


.PHONY: coverage-report
coverage-report:
	@cargo llvm-cov -q --frozen --lcov --output-path ./lcov.info
	@cat ./lcov.info | ./lcov_coverage.py

deb:
	@cargo deb --locked --strip -p orishuctl
	@cargo deb --locked --strip -p orishu-worker
	@cargo deb --locked --strip -p orishu-monitor

clean:
	@cargo clean


.PHONY: install-ctl
install-ctl:
	@cargo install --locked --path orishu-ctl

.PHONY: install-worker
install-worker:
	@cargo install --locked --path orishu-worker


.PHONY: docker-ctl
docker-ctl:
	@created=$$(date -u +%Y-%m-%dT%H:%M:%SZ); \
	revision=$$(git rev-parse HEAD); \
	GITHUB_REF_NAME=$$(git symbolic-ref --short -q HEAD || git describe --tags --exact-match 2> /dev/null || git rev-parse --short HEAD); \
	suffix=$${GITHUB_REF_NAME#release/}; \
	suffix=$$(printf '%s' "$$suffix" | tr / -); \
	version=$${suffix}-$$(printf '%s' "$$revision" | cut -c1-7); \
	$(DOCKER) build --target orishu-ctl \
		--build-arg CREATED=$${created} \
		--build-arg VERSION=$${version} \
		--build-arg REVISION=$${revision} \
		-t orishu-ctl:$${version} .
