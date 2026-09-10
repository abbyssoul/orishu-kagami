#!/usr/bin/env python3
"""Contract tests for the v2 measurement tools; no performance assertions."""
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

import formation_telemetry as common

spec = importlib.util.spec_from_file_location("curve_report", Path(__file__).with_name("summarize-formation-telemetry.py"))
reader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reader)
runner_spec = importlib.util.spec_from_file_location("curve_runner", Path(__file__).with_name("measure-formation-telemetry.py"))
runner = importlib.util.module_from_spec(runner_spec)
runner_spec.loader.exec_module(runner)
scaling_spec = importlib.util.spec_from_file_location("scaling_check", Path(__file__).with_name("check-formation-scaling.py"))
scaling = importlib.util.module_from_spec(scaling_spec)
scaling_spec.loader.exec_module(scaling)


def cell(mode="compiled_off"):
    return {"schema_version": 2, "manifest_sha256": "checkpoint", "index": 1,
            "worker_count": 3, "round": 0, "mode": mode, "status": "complete",
            "load": {"schema_version": 2, "seconds": 10, "workers": [
                {"role": role, "latency": {"requests": 100, "p95_us": 10}, "capped": False}
                for role in range(3)]},
            "resources": [{"cpu_seconds": 1, "rss_peak_kib": 1024, "swap_peak_kib": 0} for _ in range(3)],
            "tooling": {name: {"swap_peak_kib": 0} for name in ("collector", "load_generator", "runner_and_log_sink")},
            "cpu_bracket_seconds": 10.001, "proc_missed": 0,
            "logs": [{"errors": 0, "bytes": 0, "accounting": {}, "counts": {}} for _ in range(3)],
            "collector": {"schema_version": 2, "workers": [{"batches": 0, "client": 0, "peer": 0, "admission": 0} for _ in range(3)]},
            "metrics_before": None, "metrics_after": None, "scrapes": []}


class Contracts(unittest.TestCase):
    def test_convergence_can_use_remaining_setup_time_at_every_size(self):
        self.assertEqual(runner.PROFILE["convergence_budget_policy"], "remaining_whole_setup_v1")
        self.assertEqual(runner.PROFILE["observation_budget_seconds"], 10)
        self.assertEqual(runner.PROFILE["setup_budget_seconds"], 60)
        for count in (3, 10, 30):
            fixture = runner.Cell(None, count, "omitted", {}, {})
            fixture.started = 100
            clock = [130]

            def converged():
                clock[0] = 145  # Beyond ten seconds, within the original 160 deadline.
                return True

            with patch.object(runner.time, "monotonic", side_effect=lambda: clock[0]):
                self.assertTrue(fixture.poll(converged, convergence=True))

    def test_convergence_stages_cannot_renew_setup_or_accept_late_success(self):
        fixture = runner.Cell(None, 30, "metrics", {}, {})
        fixture.started = 100
        clock = [130]

        def first_stage():
            clock[0] = 159
            return True

        def second_stage():
            clock[0] = 160  # Exact deadline; a successful reply is already too late.
            return True

        with patch.object(runner.time, "monotonic", side_effect=lambda: clock[0]):
            self.assertTrue(fixture.poll(first_stage, convergence=True))
            with self.assertRaises(TimeoutError):
                fixture.poll(second_stage, convergence=True)
            with self.assertRaises(ValueError):
                fixture.poll(first_stage, setup=False, convergence=True)

    def test_ordinary_observation_retains_ten_second_and_whole_setup_limits(self):
        for began, finished in ((20, 31), (55, 60)):
            fixture = runner.Cell(None, 10, "omitted", {}, {})
            fixture.started = 0
            clock = [began]

            def late_reply():
                clock[0] = finished
                return True

            with patch.object(runner.time, "monotonic", side_effect=lambda: clock[0]):
                with self.assertRaises(TimeoutError):
                    fixture.poll(late_reply)

    def test_formation_only_failure_retains_stage_and_cleans_fixture(self):
        class FailedCell:
            def __init__(self, _args, _count, _mode, _environment, row):
                self.row, self.children, self.logs = row, ["owned-child"], []
                self.started = runner.time.monotonic()

            def form(self):
                self.row["formation_stage"] = "exact-members"
                raise TimeoutError("formation observation/setup deadline: exact-members")

        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "new-result"
            artifact = str(Path(__file__).resolve())
            arguments = ["check", "--worker", artifact, "--ctl", artifact, "--probe", artifact,
                         "--workers", "30", "--output", str(output)]
            with patch.object(scaling.sys, "argv", arguments), \
                    patch.object(scaling.curve, "competing_jobs", return_value=[]), \
                    patch.object(scaling.curve, "source_identity", return_value={}), \
                    patch.object(scaling.curve, "Cell", FailedCell), \
                    patch.object(scaling.curve, "stop", return_value=True) as stop, \
                    patch.object(scaling.signal, "setitimer"), patch("builtins.print"):
                self.assertEqual(scaling.main(), 2)
                stop.assert_called_once_with(["owned-child"])
                result = json.loads((output / "result.json").read_text())
                self.assertEqual(result["formation_stage"], "exact-members")
                self.assertEqual(result["status"], "failed")
                self.assertTrue(result["cleanup_clean"])
                self.assertFalse(result["acceptance_run"])
                with self.assertRaises(FileExistsError):
                    scaling.main()

    def test_thirty_member_cli_listing_uses_a_separate_bounded_reply_budget(self):
        fixture = runner.Cell(SimpleNamespace(ctl=Path("ctl")), 30, "omitted", {}, {})
        fixture.root = Path("/fixture")
        listing = [{"nodeId": str(role), "description": "x" * 800} for role in range(30)]

        def command(_command, **options):
            options["stdout"].write(json.dumps(listing).encode())
            return SimpleNamespace(returncode=0)

        with patch.object(runner.recipe.subprocess, "run", side_effect=command):
            self.assertEqual(fixture.cli(0, ["ls"]), listing)
            with self.assertRaises(AssertionError):
                fixture.cli(0, ["cluster", "info"])
            listing.append({"oversized": "x" * 65536})
            with self.assertRaises(AssertionError):
                fixture.cli(0, ["ls"])

    def test_join_phase_contract_is_shared_with_collector_preflight(self):
        from worker_formation_receipts import join_phase_complete
        for phase in ("connecting", "admitting", "catchingUp", "catchUpFailed"):
            self.assertFalse(join_phase_complete(phase))
        self.assertTrue(join_phase_complete("joined"))
        for phase in ("unresolved", "failedBeforeAdmission", "unknown"):
            with self.assertRaises(ValueError):
                join_phase_complete(phase)

    def test_join_wait_observes_worker_retry_without_resubmitting_admission(self):
        fixture = runner.Cell(None, 10, "omitted", {}, {})
        fixture.formation = "target"
        initial = {"formationId": "source", "sourceNodeId": "old-node"}
        reports = [{"sourceFormationId": "source", "sourceNodeId": "old-node",
                    "targetFormationId": "target", "state": {"phase": phase, "nodeId": "assigned"}}
                   for phase in ("catchingUp", "catchUpFailed", "catchingUp", "joined")]
        with patch.object(fixture, "cli", side_effect=reports) as cli, patch.object(runner.time, "sleep"):
            self.assertEqual(fixture.await_join(5, "operation", initial), reports[-1])
        self.assertEqual(cli.call_count, 4)
        self.assertTrue(all(call.args == (5, ["join-status", "operation"]) for call in cli.call_args_list))

    def test_join_retry_cannot_change_assignment_or_formation(self):
        for changed in ("node", "formation", "source"):
            fixture = runner.Cell(None, 10, "omitted", {}, {})
            fixture.formation = "target"
            initial = {"formationId": "source", "sourceNodeId": "old-node"}
            first = {"sourceFormationId": "source", "sourceNodeId": "old-node",
                     "targetFormationId": "target", "state": {"phase": "catchUpFailed", "nodeId": "assigned"}}
            second = copy.deepcopy(first)
            second["state"]["phase"] = "joined"
            if changed == "node":
                second["state"]["nodeId"] = "replacement"
            elif changed == "formation":
                second["targetFormationId"] = "replacement"
            else:
                second["sourceNodeId"] = "replacement"
            with patch.object(fixture, "cli", side_effect=[first, second]), patch.object(runner.time, "sleep"):
                with self.assertRaises(ValueError):
                    fixture.await_join(5, "operation", initial)

    def test_failed_catchup_does_not_extend_observation_deadline(self):
        fixture = runner.Cell(None, 10, "omitted", {}, {})
        fixture.formation, fixture.started = "target", 0
        report = {"sourceFormationId": "source", "sourceNodeId": "old-node",
                  "targetFormationId": "target", "state": {"phase": "catchUpFailed", "nodeId": "assigned"}}
        with patch.object(fixture, "cli", return_value=report) as cli, \
                patch.object(runner.time, "monotonic", side_effect=[0, 0, 11]), patch.object(runner.time, "sleep"):
            with self.assertRaises(TimeoutError):
                fixture.await_join(5, "operation", {"formationId": "source", "sourceNodeId": "old-node"})
        self.assertEqual(cli.call_count, 1)


    def test_competing_jobs_guard_does_not_stop_or_flag_own_workers(self):
        with patch.object(runner.subprocess, "check_output", return_value=b"worker\nprobe\npython3\ncargo\norishu_worker-ab\n"):
            self.assertEqual(runner.competing_jobs(), ["cargo", "orishu_worker-ab"])

    def test_delivery_and_shutdown_loss_are_not_omitted(self):
        row = cell("default_sample")
        counters = {f"orishu_worker_trace_{key}_total": 0 for key in common.COUNTERS}
        counters.update({f"orishu_worker_log_{key}_total": 0 for key in
                         ("accepted", "written", "queue_full", "contended", "encoding_failed", "invalid_source", "output_failed", "closed", "shutdown_dropped")})
        row.update(metrics_before=[dict(counters) for _ in range(3)], metrics_after=[dict(counters) for _ in range(3)],
                   scrapes=[{"scheduled": 40, "completed": 40, "failed": 0, "skipped": 0}] * 3)
        for log in row["logs"]:
            log["accounting"] = {key: 0 for key in common.COUNTERS}
            log["counts"] = {key: 1 for key in ("orishu.worker.ready", "orishu.worker.stopping", "orishu.worker.stopped")}
        self.assertEqual(reader.issues(row), [])
        row["logs"][0]["accounting"]["shutdown_dropped"] = 1
        row["metrics_after"][1]["orishu_worker_log_contended_total"] = 1
        found = reader.issues(row)
        self.assertIn("role_0_lifetime_trace_loss", found)
        self.assertIn("role_1_log_loss_before_shutdown", found)

    def test_accepted_bands(self):
        for value, expected in ((9.99, "normal_goal"), (10, "temporary_only_requires_disposition"),
                                (20, "temporary_only_requires_disposition"), (20.1, "outside_temporary_tolerance"),
                                (25, "outside_temporary_tolerance"), (25.1, "troubleshooting")):
            self.assertEqual(common.band(value), expected)

    def test_zero_or_invalid_denominator_is_not_improvement(self):
        for denominator in (0, -1, float("nan"), float("inf"), None):
            with self.assertRaises(ValueError):
                common.ratio(1, denominator)

    def test_non_finite_band_rejected(self):
        with self.assertRaises(ValueError):
            common.band(float("nan"))

    def test_pair_is_same_round_size_and_checkpoint(self):
        for key, value in (("round", 1), ("worker_count", 10), ("manifest_sha256", "different")):
            row = cell()
            row[key] = value
            with self.assertRaises(ValueError):
                reader.pair(row, cell())

    def test_paired_raw_totals_and_per_role_percentiles(self):
        row = cell()
        row["resources"][0]["cpu_seconds"] = 1.3
        row["load"]["workers"][2]["latency"]["p95_us"] = 12
        result = reader.pair(row, cell())
        self.assertAlmostEqual(result["cpu_change_percent"], 10)
        self.assertAlmostEqual(result["p95_change_percent"][2], 20)
        self.assertEqual(result["requests_per_second"], 30)

    def test_failed_cell_kept(self):
        self.assertEqual(reader.issues({"status": "failed"}), ["failed_or_incomplete_cell"])

    def test_missing_instruments_are_not_zero(self):
        row = cell()
        row["resources"].pop()
        with self.assertRaises(ValueError):
            reader.issues(row)

    def test_window_mismatch_rejected(self):
        row = cell()
        row["load"]["seconds"] = 2
        with self.assertRaises(ValueError):
            reader.issues(row)

    def test_cap_exhaustion_stays_invalid(self):
        row = cell()
        row["load"]["workers"][1]["capped"] = True
        self.assertIn("role_1_sample_cap_or_empty", reader.issues(row))

    def test_swap_and_schedule_skew_are_explicit(self):
        row = cell()
        row["resources"][0]["swap_peak_kib"] = 1
        row["proc_missed"] = 1
        self.assertEqual(reader.issues(row), ["measurement_schedule_skew", "process_swap"])

    def test_disabled_emission_is_not_free(self):
        row = cell()
        row["logs"][0]["bytes"] = 1
        self.assertIn("disabled_logging_emitted", reader.issues(row))

    def test_scrape_loss_kept(self):
        row = cell("metrics")
        row.update(metrics_before=[{}] * 3, metrics_after=[{}] * 3,
                   scrapes=[{"scheduled": 40, "completed": 39, "failed": 1, "skipped": 0}] * 3)
        self.assertEqual(len(reader.issues(row)), 3)

    def test_log_drain_fixed_schema_and_final_accounting(self):
        record = {"version": 1, "event": "orishu.trace.accounting", "outcome": "completed",
                  "unix_nanos": 1, "counter": "failed", "value": 7}
        drain = common.LogDrain(io.BytesIO(json.dumps(record).encode() + b"\n"))
        result = drain.finish()
        self.assertEqual(result["accounting"], {"failed": 7})
        self.assertEqual(result["errors"], 0)

    def test_log_drain_keeps_final_lifecycle_after_pipe_close(self):
        records = [{"version": 1, "event": "orishu.trace.accounting", "outcome": "completed",
                    "unix_nanos": 1, "counter": counter, "value": 0} for counter in common.COUNTERS]
        records.append({"version": 1, "event": "orishu.worker.stopped", "outcome": "completed",
                        "unix_nanos": 1})
        payload = b"".join(json.dumps(record).encode() + b"\n" for record in records)
        reader_fd, writer_fd = os.pipe()
        drain = common.LogDrain(os.fdopen(reader_fd, "rb", buffering=0))
        with os.fdopen(writer_fd, "wb") as writer:
            writer.write(payload)
        result = drain.finish()
        self.assertEqual(result["bytes"], len(payload))
        self.assertEqual(result["counts"], {"orishu.trace.accounting": 12, "orishu.worker.stopped": 1})
        self.assertEqual(result["errors"], 0)

    def test_log_malformed_oversized_partial_and_secret_fields(self):
        record = {"version": 1, "event": "orishu.worker.ready", "outcome": "completed", "unix_nanos": 1,
                  "credential": "must-not-be-exported"}
        for raw in (b"not-json\n", b"x" * 500, json.dumps(record).encode() + b"\n", b"{}"):
            result = common.LogDrain(io.BytesIO(raw)).finish()
            self.assertGreater(result["errors"], 0)
            self.assertNotIn("must-not-be-exported", json.dumps(result))

    def test_create_new_does_not_replace_results(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "result.json"
            common.write_json(path, {"kept": True})
            with self.assertRaises(FileExistsError):
                common.write_json(path, {"kept": False})
            self.assertEqual(common.read_json(path), {"kept": True})

    def test_duplicate_json_rejected(self):
        with self.assertRaises(ValueError):
            json.loads('{"x": 1, "x": 2}', object_pairs_hook=common.unique)

    def test_partial_report_never_becomes_complete(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            manifest = {"schema_version": 2, "kind": "formation-telemetry", "acceptance_run": True,
                        "profile": {"sizes": list(common.SIZES), "modes": list(common.MODES), "rounds": 6, "window_seconds": 10}}
            common.write_json(root / "manifest.json", manifest)
            row = cell("omitted")
            row["index"] = 0
            row["manifest_sha256"] = hashlib.sha256((root / "manifest.json").read_bytes()).hexdigest()
            common.write_json(root / "cell-000.json", row)
            result = reader.report(root)
            self.assertEqual(result["missing_cells"], 107)
            self.assertTrue(all(value["status"] == "incomplete" for value in result["comparisons"]))


if __name__ == "__main__":
    unittest.main()
