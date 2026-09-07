"""Negative controls for Collector receipts; no external tools or sockets."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock

from worker_otelcol_tls import CollectorTLS


spec = importlib.util.spec_from_file_location("otelcol_check", Path(__file__).with_name("check-worker-otelcol.py"))
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class TLSControlsTests(unittest.TestCase):
    def test_only_the_expected_tls_alert_is_rejection_evidence(self):
        error = check.ssl.SSLError("test alert")
        error.reason = "TLSV13_ALERT_CERTIFICATE_REQUIRED"
        check.expect_tls_alert(Mock(side_effect=error), error.reason)
        with self.assertRaisesRegex(AssertionError, "unexpected TLS refusal"):
            check.expect_tls_alert(Mock(side_effect=error), "TLSV1_ALERT_PROTOCOL_VERSION")
        with self.assertRaises(ConnectionRefusedError):
            check.expect_tls_alert(Mock(side_effect=ConnectionRefusedError()), error.reason)
        with self.assertRaisesRegex(AssertionError, "did not enforce"):
            check.expect_tls_alert(lambda: (405, b""), error.reason)

    def test_worker_credentials_are_explicit_and_separate(self):
        fixture = CollectorTLS.__new__(CollectorTLS)
        fixture.directory = Path("/private/collector-tls")
        normal = fixture.worker_arguments()
        self.assertEqual(normal, ["--tracing.ca-file", "/private/collector-tls/server-ca.crt",
                                 "--tracing.client-cert-file", "/private/collector-tls/client.crt",
                                 "--tracing.client-key-file", "/private/collector-tls/client.key"])
        self.assertEqual(fixture.worker_arguments("missing-client"), normal[:2])
        self.assertEqual(fixture.worker_arguments("wrong-server-ca")[1],
                         "/private/collector-tls/rogue-ca.crt")
        self.assertEqual(fixture.worker_arguments("wrong-client-ca")[3:],
                         ["/private/collector-tls/rogue.crt", "--tracing.client-key-file",
                          "/private/collector-tls/rogue.key"])


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name) / "traces.jsonl"
        self.span = {
            "traceId": "1" * 32, "spanId": "2" * 16,
            "name": "orishu.client.request", "kind": 2,
            "startTimeUnixNano": "1", "endTimeUnixNano": "2",
            "attributes": [{"key": "orishu.outcome", "value": {"stringValue": "completed"}}],
        }

    def document(self, spans):
        return {"resourceSpans": [{
            "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "orishu-worker"}}]},
            "scopeSpans": [{"scope": {"name": "orishu.worker", "version": "0.1.0"}, "spans": spans}],
        }]}

    def write(self, spans):
        self.path.write_bytes(json.dumps(self.document(spans)).encode() + b"\n")

    def test_valid_receipt_and_absent_file(self):
        self.assertEqual(check.read_spans(self.path), [])
        self.write([self.span])
        self.assertEqual(check.read_spans(self.path, complete=True), [self.span])

    def test_invalid_identifiers_and_context_are_refused(self):
        for field, value in (("traceId", "0" * 32), ("spanId", "F" * 16),
                             ("parentSpanId", "3" * 16), ("traceState", "vendor=value"),
                             ("name", "raw-request-path"), ("kind", 1),
                             ("endTimeUnixNano", "0"), ("status", {"message": "private"}),
                             ("unexpectedPayload", "private"), ("events", [{}]), ("links", [{}])):
            with self.subTest(field=field):
                span = copy.deepcopy(self.span)
                span[field] = value
                self.write([span])
                with self.assertRaises(AssertionError):
                    check.read_spans(self.path)

    def test_secret_and_arbitrary_attributes_are_refused(self):
        self.write([self.span])
        with self.assertRaisesRegex(AssertionError, "secret"):
            check.read_spans(self.path, (b"orishu-worker",))
        span = copy.deepcopy(self.span)
        span["attributes"].append({"key": "request.header", "value": {"stringValue": "secret"}})
        self.write([span])
        with self.assertRaises(AssertionError):
            check.read_spans(self.path)

    def test_finite_adapter_outcomes(self):
        for outcome in ("completed", "rejected", "failed", "cancelled", "arbitrary-secret"):
            span = copy.deepcopy(self.span)
            span["attributes"][0]["value"]["stringValue"] = outcome
            self.write([span])
            if outcome == "arbitrary-secret":
                with self.assertRaises(AssertionError):
                    check.read_spans(self.path)
            else:
                self.assertEqual(check.read_spans(self.path), [span])

    def test_duplicate_receipts_and_fields_are_refused(self):
        self.write([self.span, self.span])
        with self.assertRaisesRegex(AssertionError, "duplicate collected span"):
            check.read_spans(self.path)
        self.path.write_bytes(b'{"resourceSpans": [], "resourceSpans": []}\n')
        with self.assertRaisesRegex(AssertionError, "duplicate receipt field"):
            check.read_spans(self.path)

    def test_incomplete_write_is_not_a_receipt(self):
        raw = json.dumps(self.document([self.span])).encode()
        self.path.write_bytes(raw)
        self.assertEqual(check.read_spans(self.path), [])
        with self.assertRaisesRegex(AssertionError, "incomplete final"):
            check.read_spans(self.path, complete=True)
        self.path.write_bytes(raw + b"\n" + b'{"resourceSpans":')
        self.assertEqual(check.read_spans(self.path), [self.span])

    def test_byte_and_span_limits_are_enforced(self):
        self.path.write_bytes(b" " * (check.MAX_RECEIPT_BYTES + 1))
        with self.assertRaisesRegex(AssertionError, "byte budget"):
            check.read_spans(self.path)
        spans = [dict(self.span, spanId=f"{index + 1:016x}") for index in range(check.MAX_SPANS)]
        self.write(spans)
        self.assertEqual(len(check.read_spans(self.path)), check.MAX_SPANS)
        self.write(spans + [dict(self.span, spanId="f" * 16)])
        with self.assertRaisesRegex(AssertionError, "span budget"):
            check.read_spans(self.path)


if __name__ == "__main__":
    unittest.main()
