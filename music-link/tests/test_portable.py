"""Portable setup and POSIX process ownership regressions; no audio/build tools."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "music-link"))


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


setup = module("portable_setup", ROOT / "vendor/setup.py")
start = module("portable_start", ROOT / "music-link/start.py")


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="music workspace with spaces ")
        self.root = Path(self.temporary.name)
        self.manifest = {"version": 1, "projects": []}
        for name in ("one", "two"):
            repo = self.root / name
            repo.mkdir()
            def git(*args):
                return subprocess.check_output(["git", "-C", str(repo), *args], stderr=subprocess.DEVNULL)
            git("init", "-q")
            git("config", "core.autocrlf", "false")
            git("config", "user.name", "Portable test")
            git("config", "user.email", "test@example.invalid")
            (repo / "code.txt").write_bytes(b"original\n")
            git("add", "code.txt")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD").decode().strip()
            (repo / "code.txt").write_bytes(b"integrated\n")
            patch = git("diff", "--binary", "--full-index")
            folder = self.root / "vendor" / name
            (folder / "files").mkdir(parents=True)
            (folder / "changes.patch").write_bytes(patch)
            (folder / "files/code.txt").write_bytes(b"integrated\n")
            (repo / "code.txt").write_bytes(b"original\n")
            self.manifest["projects"].append({"name": name, "base": base,
                "patch_sha256": hashlib.sha256(patch).hexdigest(),
                "files": [{"path": "code.txt", "sha256": hashlib.sha256(b"integrated\n").hexdigest()}]})
        (self.root / "vendor/manifest.json").write_text(json.dumps(self.manifest))

    def tearDown(self):
        self.temporary.cleanup()

    def apply(self, **kwargs):
        with contextlib.redirect_stdout(io.StringIO()):
            setup.apply_workspace(root=self.root, **kwargs)

    def test_clean_checkout_applies_and_repeat_is_idempotent(self):
        self.apply()
        self.apply()
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"integrated\n")

    def test_check_does_not_change_source(self):
        self.apply(check_only=True)
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"original\n")

    def test_conflict_in_second_project_prevents_first_project_changes(self):
        (self.root / "two/code.txt").write_bytes(b"my local changes\n")
        with self.assertRaisesRegex(RuntimeError, "conflicting local edits"):
            self.apply()
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"original\n")
        self.assertEqual((self.root / "two/code.txt").read_bytes(), b"my local changes\n")

    def test_corrupt_vendor_copy_is_rejected_before_any_apply(self):
        (self.root / "vendor/two/files/code.txt").write_bytes(b"corrupted\n")
        with self.assertRaisesRegex(RuntimeError, "checksum mismatch"):
            self.apply()
        self.assertEqual((self.root / "one/code.txt").read_bytes(), b"original\n")

    def test_manifest_path_cannot_escape_source_copy_directory(self):
        with self.assertRaisesRegex(RuntimeError, "escapes workspace"):
            setup.checked_path(self.root, "../outside")


@unittest.skipUnless(sys.platform in ("linux", "darwin"), "POSIX process identity")
class ProcessTests(unittest.TestCase):
    def test_reused_pid_identity_does_not_authorize_stop(self):
        with subprocess.Popen([sys.executable, "-c", "import time;print('ready',flush=True);time.sleep(10)"],
                              stdout=subprocess.PIPE, text=True) as process:
            try:
                self.assertEqual(process.stdout.readline().strip(), "ready")
                identity = start.wait_for(lambda: start.process_identity(process.pid), 2, "process not ready")
                record = {"pid": process.pid, "identity": dict(identity, started="different start")}
                self.assertFalse(start.live_record(record))
                record["identity"] = identity
                self.assertEqual(start.process_identity(process.pid), identity)
                self.assertTrue(start.live_record(record))
            finally:
                process.terminate()
                process.wait()

    def test_launcher_reuses_owned_helper_and_stop_leaves_unowned_helper(self):
        with tempfile.TemporaryDirectory(prefix="music process test ") as directory:
            root = Path(directory)
            (root / "music-link").mkdir()
            launcher = start.Launcher(root)
            with contextlib.redirect_stdout(io.StringIO()):
                owned = launcher.ensure("web", sys.executable, ["-c", "import time;time.sleep(20)"], discover=False)
                try:
                    again = launcher.ensure("web", sys.executable, ["-c", "raise Exception('duplicate')"], discover=False)
                    self.assertEqual(again["pid"], owned["pid"])
                    with subprocess.Popen([sys.executable, "-c", "import time;time.sleep(20)"]) as other:
                        try:
                            identity = start.wait_for(lambda: start.process_identity(other.pid), 2, "process not ready")
                            launcher.state["processes"].append({"role": "external", "pid": other.pid,
                                                               "identity": identity, "owned": False})
                            launcher.stop()
                            start.wait_for(lambda: not start.live_record(owned), 3, "owned helper did not stop")
                            self.assertIsNone(other.poll())
                        finally:
                            other.terminate()
                            other.wait()
                finally:
                    if start.live_record(owned):
                        os.kill(owned["pid"], signal.SIGTERM)
                    try:
                        os.waitpid(owned["pid"], 0)
                    except ChildProcessError:
                        pass


if __name__ == "__main__":
    unittest.main()
