"""Headless Rust-writer/C++-reader check on Linux/macOS; no audio SDKs needed."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "music-link"))
from native_ipc import open_mapping, unlink_stopped_mapping


def main():
    assert sys.platform in ("linux", "darwin"), "Run this check on Linux or macOS"
    fixture = ROOT / "music-link/tests/ipc-producer"
    env = dict(os.environ, RUSTUP_TOOLCHAIN="stable")
    subprocess.run(["cargo", "build", "--locked", "--manifest-path", str(fixture / "Cargo.toml")], env=env, check=True)
    producer = fixture / "target/debug/lane-ipc-producer-check"
    with tempfile.TemporaryDirectory(prefix="lane native ipc ") as temporary:
        reader = Path(temporary) / "reader"
        command = ["c++", "-std=c++17", "-pthread", "-I", str(ROOT / "vendor/BespokeSynth/files/Source"),
                   str(ROOT / "music-link/tests/ipc_reader.cpp"), "-o", str(reader)]
        if sys.platform == "linux":
            command += ["-lrt"]
        subprocess.run(command, check=True)
        for crash in (False, True):
            process = subprocess.Popen([str(producer)], stdout=subprocess.PIPE, text=True)
            name = process.stdout.readline().strip()
            assert name.startswith("/rla-") and len(name) <= 31, name
            try:
                if crash:
                    watcher = subprocess.Popen([str(reader), name, "--watch-death"], stdout=subprocess.PIPE, text=True)
                    try:
                        assert watcher.stdout.readline().strip() == "opened"
                        process.kill()
                        process.wait(timeout=5)
                        assert watcher.wait(timeout=8) == 0
                    finally:
                        if watcher.poll() is None:
                            watcher.terminate()
                            watcher.wait(timeout=5)
                else:
                    subprocess.run([str(reader), name], check=True, timeout=8)
                    output = process.communicate(timeout=8)[0]
                    assert process.returncode == 0 and "muted=true restored=true" in output, output
                try:
                    with open_mapping(name):
                        raise AssertionError("POSIX name was not cleaned up")
                except OSError:
                    pass
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=5)
                unlink_stopped_mapping(name, process.pid)
    print("PASS Rust/C++ native IPC, monitor lease expiry, normal unlink and dead-producer cleanup")


if __name__ == "__main__":
    main()
