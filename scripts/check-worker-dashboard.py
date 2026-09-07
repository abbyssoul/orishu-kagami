#!/usr/bin/env python3
"""Real Prometheus console/worker checks plus synthetic panel-query fixtures."""

import argparse
import contextlib
from html.parser import HTMLParser
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import signal
import shutil
import socket
import subprocess
import tempfile
import urllib.parse


ROOT = Path(__file__).resolve().parent.parent
CONSOLES = ROOT / "etc/prometheus-consoles"
PANEL_COUNT = 37
RESPONSE_LIMIT = 256 * 1024


class Dashboard(HTMLParser):
    def __init__(self, body):
        super().__init__()
        self.panels = {}
        self.target_state = None
        self.links = []
        self.current = None
        self.scripts = 0
        self.feed(body.decode("utf-8"))

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "script":
            self.scripts += 1
        if "data-target-state" in attrs:
            assert self.target_state is None, "multiple target-state banners"
            self.target_state = attrs["data-target-state"]
        if tag == "a":
            self.links.append(attrs["href"])
        if tag == "tr" and "data-panel" in attrs:
            self.current = attrs["data-panel"]
            assert self.current not in self.panels, "duplicate panel"
            self.panels[self.current] = {"query": attrs["data-query"]}
        if tag == "td" and "data-state" in attrs:
            self.panels[self.current].update(attrs)

    def handle_endtag(self, tag):
        if tag == "tr":
            self.current = None


def query_fixtures(panels, instance, catalogue):
    """Use the queries rendered by the actual template, not copied expressions."""
    names = {name for panel in panels.values() for name in re.findall(
        r"orishu_worker_[a-z0-9_]+", panel["query"])}
    assert names <= catalogue, "dashboard selects an unknown worker metric"
    cases = []
    bounds = ("0.001", "0.005", "0.025", "0.1", "0.5", "1", "5", "+Inf")
    for mode in ("known-values", "idle", "reset", "unready", "zero-capacity", "absent", "stale", "other-target"):
        label_instance = "different:1" if mode == "other-target" else instance
        labels = f'job="orishu-worker",instance="{label_instance}"'
        inputs = [{"series": "up{" + labels + "}", "values": "1x6"}]
        for name in sorted(names):
            if mode == "absent":
                continue
            if name.endswith("_bucket"):
                for bound in bounds:
                    values = "0+60x6" if mode == "known-values" else "0+60x2 stale" if mode == "stale" else "0x6"
                    inputs.append({"series": name + "{" + labels + f',le="{bound}"' + "}", "values": values})
            else:
                if mode == "stale":
                    values = "0+60x2 stale"
                elif name.endswith(("_total", "_count")):
                    values = "0+60x6" if mode == "known-values" else "9x2 0x4" if mode == "reset" else "0x6"
                elif name.endswith("_slots_capacity"):
                    values = "0x6" if mode == "zero-capacity" else "64x6"
                elif name.endswith("_slots_in_use"):
                    values = "64x6" if mode == "known-values" else "0x6"
                else:
                    values = "0x6" if mode == "unready" and name == "orishu_worker_ready" else "1x6"
                inputs.append({"series": name + "{" + labels + "}", "values": values})
        checks = []
        for panel_id, panel in panels.items():
            expression = panel["query"]
            missing = mode == "other-target" or (mode in ("absent", "stale") and panel_id != "scrape")
            missing |= mode == "zero-capacity" and panel_id.startswith("capacity-")
            sample_labels = labels
            if panel_id in ("scrape", "owner", "ready", "startup"):
                metric = "up" if panel_id == "scrape" else {"owner": "orishu_worker_owner_responsive", "ready": "orishu_worker_ready", "startup": "orishu_worker_startup_complete"}[panel_id]
                sample_labels = f'__name__="{metric}",' + labels
            if panel_id == "client-p95":
                # NaN must not turn into zero: self-equality filters NaN while
                # retaining a finite histogram estimate in the positive case.
                expression = f"({expression}) == ({expression})"
                missing |= mode != "known-values"
                value = 0.00095
            elif panel_id.startswith("capacity-"):
                value = 100 if mode == "known-values" else 0
            elif panel_id in ("scrape", "owner", "ready", "startup"):
                value = 0 if mode == "unready" and panel_id == "ready" else 1
            else:
                value = 1 if mode == "known-values" else 0
            checks.append({"expr": expression, "eval_time": "5m", "exp_samples": [] if missing else [
                {"labels": "{" + sample_labels + "}", "value": value}]})
        cases.append({"name": mode, "interval": "1m", "input_series": inputs, "promql_expr_test": checks})
    return {"evaluation_interval": "1m", "tests": cases}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, default=ROOT / "target/debug/orishu-worker")
    parser.add_argument("--ctl", type=Path, default=ROOT / "target/debug/orishuctl")
    parser.add_argument("--promtool", type=Path, required=True)
    parser.add_argument("--prometheus", type=Path, required=True)
    parser.add_argument("--browser", type=Path, help="optional local Chromium-compatible screenshot smoke")
    parser.add_argument("--screenshot-dir", type=Path, help="new directory for optional screenshots; never overwrite")
    args = parser.parse_args()
    if bool(args.browser) != bool(args.screenshot_dir):
        parser.error("--browser and --screenshot-dir must be selected together")
    node = shutil.which("node") if args.browser else None
    if args.browser and not node:
        parser.error("optional browser captures require Node.js on PATH")
    spec = importlib.util.spec_from_file_location("worker_prometheus", ROOT / "scripts/check-worker-prometheus.py")
    harness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(harness)
    for tool in (args.promtool, args.prometheus):
        version = harness.run([str(tool.resolve()), "--version"], text=True)
        assert "version 3.5.0 " in version.stdout + version.stderr
    environment = {key: value for key, value in os.environ.items() if not key.startswith("ORISHU_")}
    with tempfile.TemporaryDirectory(prefix="orishu-dashboard-") as temporary, contextlib.ExitStack() as stack:
        root = Path(temporary)
        collector = stack.enter_context(harness.trace_collector())
        ports = [harness.port() for _ in range(3)]
        assert len(set(ports)) == 3, "port reservation collision"
        dead = stack.enter_context(socket.socket())
        dead.bind(("127.0.0.1", 0))  # Retain this port without listening.
        dead_instance = "127.0.0.1:" + str(dead.getsockname()[1])
        workers = []
        for index in range(2):
            command = [str(args.worker.resolve()), "--state-dir", str(root / str(index)),
                       "--listen.clients", str(root / f"{index}.sock"), "--observability.enabled", "true",
                       "--observability.bind", f"127.0.0.1:{ports[index]}"]
            if index == 1:
                command += ["--tracing.enabled", "true", "--tracing.endpoint", collector.endpoint,
                            "--tracing.sample-ppm", "1000000", "--tracing.batch-size", "1",
                            "--tracing.export-max-bytes", "1024", "--tracing.export-timeout-ms", "1000",
                            "--tracing.shutdown-timeout-ms", "100"]
            workers.append(stack.enter_context(harness.process(command, environment)))
            harness.poll(lambda: harness.get(ports[index], "/readyz", 1024)[0] == 200, workers)

        def summary(index):
            result = harness.run([str(args.ctl.resolve()), "--host", str(root / f"{index}.sock"),
                                  "--timeout", "2s", "--output", "json", "cluster", "info"], env=environment)
            return json.loads(result.stdout)

        original = [summary(index) for index in range(2)]
        config = root / "prometheus.yml"
        config.write_text(json.dumps({"global": {"scrape_interval": "1s", "scrape_timeout": "1s"},
            "scrape_configs": [
                {"job_name": "orishu-worker", "static_configs": [{"targets": [
                    f"127.0.0.1:{ports[0]}", f"127.0.0.1:{ports[1]}", dead_instance]}]},
                {"job_name": "orishu-dashboard-ambiguous", "static_configs": [
                    {"targets": [f"127.0.0.1:{ports[index]}"], "labels": {
                        "instance": "ambiguous:1", "replica": str(index)}} for index in range(2)]}
            ]}), encoding="utf-8")
        harness.run([str(args.promtool.resolve()), "check", "config", str(config)])
        assert not list(CONSOLES.glob("*.lib")), "self-contained console must not acquire legacy libraries"
        server = stack.enter_context(harness.process([str(args.prometheus.resolve()), "--config.file", str(config),
            "--web.listen-address", f"127.0.0.1:{ports[2]}", "--storage.tsdb.path", str(root / "tsdb"),
            "--web.console.templates", str(CONSOLES), "--web.console.libraries", str(CONSOLES),
            "--query.timeout", "2s"]))

        def render(instance=None, job="orishu-worker", extra=None):
            params = [("job", job)] + ([] if instance is None else [("instance", instance)]) + (extra or [])
            url = "/consoles/orishu-worker.html?" + urllib.parse.urlencode(params)
            status, headers, body = harness.get(ports[2], url, RESPONSE_LIMIT)
            assert status == 200, f"console HTTP {status}: {body[:2048].decode('utf-8', errors='replace')}"
            assert {key.lower(): value for key, value in headers.items()}["content-type"].startswith("text/html")
            for index in range(2):
                assert (root / str(index) / "operator.token").read_bytes().strip() not in body
            dashboard = Dashboard(body)
            assert dashboard.scripts == 0, "console must not load legacy/external scripts"
            return dashboard, url

        def query(expression):
            url = "/api/v1/query?" + urllib.parse.urlencode({"query": expression})
            status, _, body = harness.get(ports[2], url, 16384)
            assert status == 200
            data = json.loads(body)
            assert data["status"] == "success" and not data.get("warnings")
            return data["data"]["result"]

        harness.poll(lambda: harness.get(ports[2], "/-/ready", 4096)[0] == 200, workers + [server])
        harness.poll(lambda: len(query('count_over_time(up{job="orishu-worker"}[5m]) >= 2')) == 3, workers + [server])
        board, _ = render()
        assert board.target_state == "unselected" and not board.panels
        for index in range(2):
            board, url = render(f"127.0.0.1:{ports[index]}")
            assert board.target_state == "up" and len(board.panels) == PANEL_COUNT
            for panel_id, panel in board.panels.items():
                if panel["data-state"] == "value":
                    assert math.isfinite(float(panel["data-value"]))
                if panel_id.startswith("trace-"):
                    assert panel["data-state"] == ("missing" if index == 0 else "value")
            for panel_id in ("scrape", "owner", "ready", "startup"):
                assert board.panels[panel_id]["data-value"] == "1"
            for panel_id in ("capacity-peer_inbound_tls", "capacity-peer_inbound_connection"):
                assert board.panels[panel_id]["data-state"] == "missing", "unconfigured listener is not 0% utilized"
            if index == 0:
                suite = query_fixtures(board.panels, f"127.0.0.1:{ports[0]}", harness.NAMES | harness.TRACE_NAMES)
                fixtures = root / "dashboard.test.yml"
                fixtures.write_text(json.dumps(suite), encoding="utf-8")
                result = subprocess.run([str(args.promtool.resolve()), "test", "rules", str(fixtures)],
                                        capture_output=True, text=True, timeout=10)
                assert result.returncode == 0, (result.stdout + result.stderr)[:8192]
                for href in board.links:
                    if href.startswith("/"):
                        # Prometheus 3 redirects the built-in /graph link to its
                        # current expression UI. Follow only bounded local paths.
                        for _ in range(3):
                            status, headers, _ = harness.get(ports[2], href, RESPONSE_LIMIT)
                            if status == 200:
                                break
                            assert status in (301, 302, 303, 307, 308), f"dashboard navigation HTTP {status}"
                            headers = {key.lower(): value for key, value in headers.items()}
                            target = urllib.parse.urlsplit(headers["location"])
                            assert not target.scheme and not target.netloc and target.path.startswith("/")
                            href = urllib.parse.urlunsplit(target)
                        else:
                            raise AssertionError("dashboard redirect limit")

        for instance, job, state in ((dead_instance, "orishu-worker", "down"),
                                     ("missing:1", "orishu-worker", "missing"),
                                     ("ambiguous:1", "orishu-dashboard-ambiguous", "ambiguous")):
            board, _ = render(instance, job)
            assert board.target_state == state and len(board.panels) == PANEL_COUNT
            assert all(panel["data-state"] == "unavailable" for name, panel in board.panels.items() if name != "scrape")
        for instance, job, extra in (("x" * 257, "orishu-worker", None),
                                     ("host:1", "x" * 129, None),
                                     ('\"><script>alert(1)</script>', "orishu-worker", None),
                                     ("host:1", "orishu-worker", [("job", "other-job")]),
                                     ("host:1", "orishu-worker", [("instance", "other:2")])):
            board, _ = render(instance, job, extra)
            assert board.target_state == "invalid" and not board.panels
        board, _ = render(":" * 256, ":" * 128)
        assert board.target_state == "missing", "maximum valid literal selection should remain bounded"

        def trace_rate(panel):
            board, _ = render(f"127.0.0.1:{ports[1]}")
            return float(board.panels[panel].get("data-value", "0"))

        # A loss before the first scrape is a nonzero total, not an observed
        # positive rate. Produce another read-only operation after the baseline.
        assert summary(1)["formationId"] == original[1]["formationId"]
        harness.poll(lambda: trace_rate("trace-failed") > 0, workers + [server])
        collector.recover()
        assert summary(1)["formationId"] == original[1]["formationId"]
        harness.poll(lambda: trace_rate("trace-accepted") > 0, workers + [server])
        assert all(summary(index)["sourceNodeId"] == original[index]["sourceNodeId"] for index in range(2))
        if args.browser:
            screenshots = args.screenshot_dir.resolve()
            screenshots.mkdir(mode=0o700)  # Refuse an existing path.
            url = f"http://127.0.0.1:{ports[2]}/consoles/orishu-worker.html?instance=127.0.0.1:{ports[1]}"
            command = [node, str(ROOT / "scripts/capture-worker-dashboard.mjs"),
                       str(args.browser.resolve()), url, str(root / "browser-profile"), str(screenshots)]
            browser = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True)
            try:
                _, error = browser.communicate(timeout=10)
                assert browser.returncode == 0, "browser screenshot failed: " + error[:2048].decode("utf-8", errors="replace")
            finally:
                try:
                    os.killpg(browser.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    browser.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(browser.pid, signal.SIGKILL)
                    browser.wait(timeout=3)
                    raise AssertionError("browser exceeded cleanup deadline")
            print("Screenshots captured; visual inspection still required:", screenshots)
    print(f"PASS: {PANEL_COUNT} rendered panels, eight synthetic query scenarios, two real worker modes, failed/missing/ambiguous targets, hostile selectors and collector recovery")


if __name__ == "__main__":
    main()
