#!/usr/bin/env python3
"""Late-reply recovery through the real coordinator pipe/framing path."""
import json
import os
import time
from types import SimpleNamespace
import unittest

import pi_lab_experiment as experiment


class RecoveryContracts(unittest.TestCase):
    def test_timeout_then_late_reply_is_never_used_as_cleanup_receipt(self):
        self.check_timeout(0.02)

    def test_longer_timeout_does_not_restore_reply_pairing(self):
        self.check_timeout(0.1)

    def test_partial_reply_timeout_also_forbids_reuse(self):
        self.check_timeout(0.02, partial=True)

    def check_timeout(self, timeout, partial=False):
        request_read, request_write = os.pipe()
        reply_read, reply_write = os.pipe()
        with os.fdopen(request_read, 'rb', buffering=0) as requests, \
             os.fdopen(request_write, 'wb', buffering=0) as outgoing, \
             os.fdopen(reply_read, 'rb', buffering=0) as incoming, \
             os.fdopen(reply_write, 'wb', buffering=0) as replies:
            remote = object.__new__(experiment.Remote)
            remote.child = SimpleNamespace(stdin=outgoing, stdout=incoming)
            remote.deadline = time.monotonic() + timeout
            closed = []
            def close():
                closed.append(True)
                outgoing.close()  # Real EOF to the session's request pipe.
                incoming.close()
            remote.close = close
            if partial:
                replies.write(b'{"ok":')
            with self.assertRaises(ValueError):
                remote.call('info')
            late = b'true,"result":{"formationId":"late-info-reply"}}\n'
            replies.write(late if partial else b'{"ok":' + late)
            result = remote.stop()
            self.assertEqual(result, {'clean': False, 'reason': 'session_reply_unverified'})
            self.assertEqual(closed, [True])
            # No stop RPC may follow a lost reply boundary. A new request could
            # consume the pending info reply as its own acknowledgement.
            sent = requests.read()
            self.assertEqual([json.loads(line)['operation'] for line in sent.splitlines()], ['info'])

    def test_healthy_session_still_requests_and_returns_cleanup(self):
        remote = object.__new__(experiment.Remote)
        sent, closed = [], []
        remote.write = sent.append
        remote.read = lambda _: {'ok': True, 'result': {'clean': True}}
        remote.close = lambda: closed.append(True)
        self.assertEqual(remote.stop(), {'clean': True})
        self.assertEqual(sent, [{'operation': 'stop', 'arguments': {}}])
        self.assertEqual(closed, [True])

    def test_rejected_reply_or_uncertain_write_forbids_later_rpc(self):
        for fail_write in (True, False):
            remote = object.__new__(experiment.Remote)
            sent = []
            def write(value):
                sent.append(value)
                if fail_write:
                    raise BrokenPipeError('uncertain write')
            remote.write = write
            remote.read = lambda _: {'ok': False, 'error_type': 'ValueError'}
            with self.assertRaises((ValueError, BrokenPipeError)):
                remote.call('info')
            self.assertTrue(remote.poisoned)
            with self.assertRaises(ValueError):
                remote.call('stop')
            self.assertEqual(len(sent), 1)


if __name__ == '__main__':
    unittest.main()
