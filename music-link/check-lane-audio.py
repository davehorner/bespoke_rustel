"""Hardware-free native producer/IPC check. Stops only processes it creates."""
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time
import urllib.request
import sys
from native_ipc import open_mapping, unlink_stopped_mapping

ROOT = Path(__file__).resolve().parent.parent
BASE = "http://127.0.0.1:9614/sessions"
SIZE = 4096 + 15 * 16384 * 2 * 8
FIRST = '$: note("c3 ~ ~ ~ ~ ~ ~ ~").s("sine").gain(0.08).pan(-0.6).delay(0.7).delaytime(0.15).delayfeedback(0.6)\n'
SECOND = '$: note("g4 ~ ~ ~").s("triangle").gain(0.05).pan(0.6)\n'


def sessions():
    with urllib.request.urlopen(BASE, timeout=1) as response:
        return json.load(response)


def wait_for(check, seconds=12):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        try:
            result = check()
            if result:
                return result
        except (OSError, ValueError):
            pass
        time.sleep(0.05)
    raise AssertionError("Timed out waiting for live lane audio")


def main():
    owned = []
    exported_mapping = None
    producer_pid = None
    logs = ROOT / "music-link/logs"
    logs.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="lane-audio-", dir=logs) as temporary:
        score = Path(temporary) / "probe.strudel"
        score.write_text(FIRST + SECOND, encoding="utf-8")
        with (logs / "lane-audio-check.log").open("w", encoding="utf-8") as log:
            def launch(args, env=None, cwd=ROOT):
                options = {}
                if os.name == "nt":
                    startup = subprocess.STARTUPINFO()
                    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
                    startup.wShowWindow = 0
                    options = {"creationflags": subprocess.CREATE_NO_WINDOW, "startupinfo": startup}
                else:
                    options = {"start_new_session": True}
                process = subprocess.Popen(args, cwd=cwd, env=env, stdout=log, stderr=log, **options)
                owned.append(process)
                return process
            try:
                try:
                    sessions()
                except OSError:
                    launch([str(ROOT / ("music-link/target/debug/music-hub.exe" if os.name == "nt" else "music-link/target/debug/music-hub"))])
                    wait_for(lambda: sessions() is not None)
                env = dict(os.environ, RUSTEL_MUSIC_LINK="1", RUSTEL_LANE_AUDIO="1")
                if os.name == "nt":
                    rustel = ROOT / "rustel/target/debug/rustel.exe"
                else:
                    from start_music import RUSTEL_OUTPUT
                    rustel = ROOT / "rustel/target" / RUSTEL_OUTPUT / "rustel"
                process = launch([str(rustel), "play", str(score),
                                  "--watch", "--no-update-check", "--no-input", "--output", "silent"], env)
                def current():
                    return next((s for s in sessions() if s["session"].startswith(f"rustel-{process.pid}-")), None)
                snapshot = wait_for(lambda: (s := current()) and s.get("lane_audio") and len(s["lane_audio"]["lanes"]) == 2 and s)
                producer_pid = process.pid
                stream = snapshot["lane_audio"]
                exported_mapping = stream["mapping"]
                slots = stream["lanes"]
                assert len(set(slots.values())) == 2
                with open_mapping(stream["mapping"]) as memory:
                    assert struct.unpack_from("<II", memory) == (0x524C4131, 1)
                    assert struct.unpack_from("<II", memory, 12) == (15, 16384)
                    rate = wait_for(lambda: struct.unpack_from("<I", memory, 8)[0])
                    assert rate == 48000, rate
                    peaks = {slot: 0.0 for slot in slots.values()}
                    stereo = {slot: 0.0 for slot in slots.values()}
                    first_head = struct.unpack_from("<Q", memory, 24)[0]
                    deadline = time.monotonic() + 4
                    while time.monotonic() < deadline:
                        head = struct.unpack_from("<Q", memory, 24)[0]
                        if head >= 1024:
                            for slot in peaks:
                                for frame in range(head - 1024, head):
                                    at = 4096 + ((slot * 16384 + (frame & 16383)) * 2) * 8
                                    a, b = struct.unpack_from("<QQ", memory, at)
                                    assert a >> 32 == frame & 0xFFFFFFFF
                                    assert b >> 32 == frame & 0xFFFFFFFF
                                    left = struct.unpack("<f", struct.pack("<I", a & 0xFFFFFFFF))[0]
                                    right = struct.unpack("<f", struct.pack("<I", b & 0xFFFFFFFF))[0]
                                    peaks[slot] = max(peaks[slot], abs(left), abs(right))
                                    stereo[slot] = max(stereo[slot], abs(left - right))
                        time.sleep(0.04)
                    assert all(peak > 0.001 for peak in peaks.values()), peaks
                    assert all(delta > 0.001 for delta in stereo.values()), stereo
                    assert struct.unpack_from("<Q", memory, 24)[0] > first_head
                    consumed = {}
                    if "--bespoke" in sys.argv:
                        layout = Path(temporary) / "layout.json"
                        preferences = Path(temporary) / "prefs.json"
                        modules = []
                        for i, (lane, slot) in enumerate(slots.items()):
                            modules += [{"name": f"lane {i}", "type": "rustelaudio", "position": [30, 80 + i * 100],
                                         "binding": snapshot["session"] + "/" + lane, "target": f"test sink {i}"},
                                        {"name": f"test sink {i}", "type": "gain", "position": [440, 80 + i * 100]}]
                        layout.write_text(json.dumps({"modules": modules, "zoomlocations": []}), encoding="utf-8")
                        # Demo/test purposes only: expose the test patch without the welcome overlay.
                        preferences.write_text(json.dumps({"layout": str(layout).replace("\\", "/"),
                                                           "audio_input_device": "none", "audio_output_device": "auto",
                                                           "show_welcome_screen": False}), encoding="utf-8")
                        # Sinks have no output connection: this tests native callback
                        # consumption without sending the probe to the speakers.
                        if os.name == "nt":
                            executable = ROOT / "BespokeSynth/ignore/music-build/Source/BespokeSynth_artefacts/Release/BespokeSynth.exe"
                        else:
                            from start_music import bespoke_candidates
                            executable = next(p for p in bespoke_candidates(ROOT, sys.platform) if p.is_file())
                        launch([str(executable), str(preferences)], cwd=ROOT / "BespokeSynth")
                        for slot in slots.values():
                            wait_for(lambda: struct.unpack_from("<Q", memory, 72 + slot * 64)[0] > 4096, seconds=20)
                            consumed[slot] = struct.unpack_from("<Q", memory, 72 + slot * 64)[0]
                    # Removing a lane must not recycle its slot into its sibling.
                    generation = snapshot["generation"]
                    score.write_text(SECOND, encoding="utf-8")
                    updated = wait_for(lambda: (s := current()) and s["generation"] != generation and len(s["lanes"]) == 1 and s)
                    assert updated["lane_audio"]["mapping"] == stream["mapping"]
                    assert updated["lane_audio"]["lanes"] == slots
                    print(json.dumps({"result": "PASS", "sample_rate": rate, "lane_peaks": peaks,
                                      "stereo_difference": stereo, "retired_slots_preserved": True,
                                      "bespoke_callback_frames": consumed}))
            finally:
                for process in reversed(owned):
                    if process.poll() is None:
                        process.terminate()
                    process.wait(timeout=8)
                if exported_mapping is not None:
                    unlink_stopped_mapping(exported_mapping, producer_pid)


if __name__ == "__main__":
    main()
