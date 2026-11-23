"""Two silent Rustel instances: demos, tempo RPC and independent session files."""
from pathlib import Path
import json
import os
import subprocess
import tempfile
import time
import urllib.request
import uuid
from session_score import create_session_score

ROOT = Path(__file__).resolve().parent.parent
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def request(path, data=None):
    payload = None if data is None else json.dumps(data).encode()
    with HTTP.open(urllib.request.Request("http://127.0.0.1:9614"+path, data=payload,
                                        headers={"Content-Type": "application/json"}), timeout=3) as response:
        return json.load(response)


def wait_for(check, seconds=20):
    deadline = time.monotonic()+seconds
    while time.monotonic() < deadline:
        try:
            value = check()
            if value:
                return value
        except (OSError, ValueError):
            pass
        time.sleep(0.1)
    raise AssertionError("Timed out waiting for session-file update")


def main():
    owned = []
    suffix = ".exe" if os.name == "nt" else ""
    (ROOT / "music-link/target").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="session files e2e ", dir=ROOT / "music-link/target") as temporary:
        folder = Path(temporary)
        demos = folder / "strudel"
        demos.mkdir()
        one = 'setcpm(120/4); const bpm = 120; $: note("c3*4").s("sine").gain(0.01)\n'
        two = 'setcpm(90/4); const bpm = 90; $: note("g4*4").s("sine").gain(0.01)\n'
        (demos / "one.strudel").write_text(one)
        (demos / "two.strudel").write_text(two)
        log_path = ROOT / "music-link/target/session-files-e2e.log"
        with log_path.open("w") as log:
            def launch(args, env=None):
                options = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {"start_new_session": True}
                process = subprocess.Popen([str(a) for a in args], cwd=ROOT, env=env, stdout=log, stderr=log, **options)
                owned.append(process)
                return process
            try:
                try:
                    request("/sessions")
                except OSError:
                    launch([ROOT / ("music-link/target/debug/music-hub"+suffix)])
                    wait_for(lambda: request("/sessions") is not None)
                processes = []
                files = []
                for _ in range(2):
                    score = create_session_score(demos / "one.strudel", folder)
                    env = dict(os.environ, RUSTEL_MUSIC_LINK="1", MUSIC_LINK_ROOT=str(folder),
                               RUSTEL_SESSION_FILE=str(score), RUSTEL_DEMO_NAME="one.strudel")
                    process = launch([ROOT / ("rustel/target/debug/rustel"+suffix), "play", score,
                                      "--watch", "--no-update-check", "--no-input", "--output", "silent"], env)
                    processes.append(process)
                    files.append(score)
                def current(index):
                    return next((s for s in request("/sessions") if s["session"].startswith(f"rustel-{processes[index].pid}-")), None)
                first = wait_for(lambda: (s := current(0)) and s.get("session_file") and s)
                wait_for(lambda: current(1))
                assert Path(first["session_file"]).resolve() == files[0].resolve(), first
                assert first["demos"] == ["one.strudel", "two.strudel"], first["demos"]
                def control(index, **fields):
                    snapshot = current(index)
                    result = request("/control", dict(request=uuid.uuid4().hex, session=snapshot["session"],
                                                     generation=snapshot["generation"], lane="", **fields))
                    assert result["accepted"], result
                control(0, tempo={"bpm": 135, "top": 6, "bottom": 8})
                wait_for(lambda: "timeSignatureTop = 6" in files[0].read_text() and "const bpm = 135" in files[0].read_text())
                snapshot = wait_for(lambda: (s := current(0)) and s["tempo"]["top"] == 6 and s)
                assert abs(snapshot["cps"]-0.75) < 1e-7, snapshot["cps"]
                definitions = snapshot["definitions"]
                for name, value in [("bpm", 135), ("timeSignatureTop", 6), ("timeSignatureBottom", 8)]:
                    matches = [d for d in definitions if d["name"] == name]
                    assert len(matches) == 1 and matches[0]["value"] == value, (name, matches)
                    assert matches[0]["history"], (name, "missing numeric history")
                    assert all(p["name"] != name for lane in snapshot["lanes"] for p in lane["parts"]), name
                generation = snapshot["generation"]
                time.sleep(1)
                assert current(0)["generation"] == generation, "persisting a control must not trigger a second watch reload"
                assert files[1].read_text() == one, "tempo update touched another instance"
                (demos / "added.strudel").write_text('$: s("bd")\n')
                wait_for(lambda: "added.strudel" in current(0)["demos"])
                control(0, demo="two.strudel")
                wait_for(lambda: current(0)["demo"] == "two.strudel" and "g4*4" in files[0].read_text())
                assert files[1].read_text() == one
                assert (demos / "one.strudel").read_text() == one
                assert (demos / "two.strudel").read_text() == two
                assert Path(current(0)["session_file"]).resolve() == files[0].resolve()
                print("PASS two independent named session files, live demo catalog, 135 BPM / 6/8 rewrite, demo switching, templates unchanged")
            finally:
                for process in reversed(owned):
                    if process.poll() is None:
                        process.terminate()
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait(timeout=5)


if __name__ == "__main__":
    main()
