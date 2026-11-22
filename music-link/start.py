"""macOS/Linux workspace launcher (Python 3.9+). Windows uses start.ps1."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
import webbrowser
from native_ipc import unlink_stopped_mapping

ROOT = Path(__file__).resolve().parent.parent
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def process_identity(pid):
    """PID plus start identity and executable; never stop a recycled PID."""
    try:
        if sys.platform.startswith("linux"):
            proc = Path("/proc") / str(pid)
            fields = (proc / "stat").read_text().rsplit(")", 1)[1].split()
            if fields[0] == "Z":
                return None
            return {"started": fields[19], "executable": str((proc / "exe").resolve(strict=True))}
        if sys.platform == "darwin":
            result = subprocess.run(["ps", "-p", str(pid), "-o", "lstart=", "-o", "comm="],
                                    capture_output=True, text=True, timeout=2)
            # lstart has five whitespace-separated fields; the remainder is comm.
            parts = result.stdout.strip().split(None, 5)
            if result.returncode or len(parts) != 6:
                return None
            return {"started": " ".join(parts[:5]), "executable": str(Path(parts[5]).resolve())}
    except (OSError, ValueError, IndexError, subprocess.TimeoutExpired):
        pass
    return None


def live_record(record):
    return process_identity(record["pid"]) == record["identity"]


def find_existing(executable):
    executable = str(executable.resolve())
    if sys.platform.startswith("linux"):
        pids = (int(p.name) for p in Path("/proc").iterdir() if p.name.isdigit())
    else:
        result = subprocess.run(["ps", "-axo", "pid=,comm="], capture_output=True, text=True, check=True)
        pids = [int(line.split(None, 1)[0]) for line in result.stdout.splitlines()
                if len(line.split(None, 1)) == 2 and Path(line.split(None, 1)[1]).name == Path(executable).name]
    for pid in pids:
        identity = process_identity(pid)
        if identity and identity["executable"] == executable:
            return {"pid": pid, "identity": identity, "owned": False}
    return None


def hub_state():
    try:
        with HTTP.open("http://127.0.0.1:9614/sessions", timeout=1) as response:
            sessions = json.load(response)
        if not isinstance(sessions, list) or any(not isinstance(s, dict) or s.get("version") != 1
                                                or not s.get("session") for s in sessions):
            return None
        return sessions
    except (OSError, ValueError):
        return None


def wait_for(check, seconds, failure):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.2)
    raise RuntimeError(failure)


def run(arguments, cwd=ROOT, env=None):
    if not shutil.which(str(arguments[0])) and not Path(arguments[0]).is_file():
        raise RuntimeError(f"Missing build tool: {arguments[0]}; see music-link/README.md")
    print("Building:", " ".join(map(str, arguments)), flush=True)
    subprocess.run(list(map(str, arguments)), cwd=cwd, env=env, check=True)


def bespoke_candidates(root, platform):
    artifacts = root / "BespokeSynth/ignore/music-build-posix/Source/BespokeSynth_artefacts"
    suffix = Path("BespokeSynth.app/Contents/MacOS/BespokeSynth") if platform == "darwin" else Path("BespokeSynth")
    return [artifacts / "Release" / suffix, artifacts / suffix]


def build_viewer(root, rebuild):
    viewer = root / "nightshade-viewer"
    if (viewer / "dist/index.html").is_file() and not rebuild:
        return
    for tool in ("cargo", "rustup", "npm", "trunk"):
        if not shutil.which(tool):
            raise RuntimeError(f"Web viewer requires {tool}. Install prerequisites in music-link/README.md")
    env = dict(os.environ, RUSTUP_TOOLCHAIN="stable", NO_COLOR="true")
    tools = root / "music-link/tools"
    bindgen = tools / "bin/wasm-bindgen"
    if not bindgen.is_file():
        run(["cargo", "install", "wasm-bindgen-cli", "--version", "0.2.115", "--root", tools, "--locked"], root, env)
    env["PATH"] = str(bindgen.parent) + os.pathsep + env.get("PATH", "")
    run(["rustup", "target", "add", "wasm32-unknown-unknown"], root, env)
    if not (viewer / "node_modules").is_dir():
        run(["npm", "ci"], viewer, env)
    run(["cargo", "build", "-p", "worker", "--target", "wasm32-unknown-unknown"], viewer, env)
    run([bindgen, "--target", "web", "--out-dir", "runtime", "--out-name", "engine",
         "target/wasm32-unknown-unknown/debug/worker.wasm"], viewer, env)
    run(["npm", "run", "build:css"], viewer, env)
    run(["trunk", "build"], viewer, env)


@contextmanager
def launcher_lock(path):
    import fcntl
    with path.open("a") as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError("Another music launcher is working; wait for it to finish") from None
        yield


class Launcher:
    def __init__(self, root=ROOT):
        self.root = root.resolve()
        self.folder = self.root / "music-link"
        self.state_path = self.folder / "launcher-posix.json"
        self.logs = self.folder / "logs"
        self.children = []
        self.state = {"version": 1, "workspace": str(self.root), "processes": [], "web_opened": False}
        if self.state_path.exists():
            self.state = json.loads(self.state_path.read_text())
            if self.state.get("version") != 1 or self.state.get("workspace") != str(self.root):
                raise RuntimeError("Launcher state belongs to another workspace")

    def save(self):
        temporary = self.state_path.with_suffix(".json.tmp")
        temporary.write_text(json.dumps(self.state, indent=2) + "\n")
        temporary.replace(self.state_path)

    def ensure(self, role, executable, arguments=(), env=None, cwd=None, discover=True):
        previous = next((r for r in self.state["processes"] if r["role"] == role and live_record(r)), None)
        record = previous or (find_existing(Path(executable)) if discover else None)
        if record:
            print(f"Reusing {role} (PID {record['pid']}); its existing settings stay in effect")
        else:
            self.logs.mkdir(exist_ok=True)
            with (self.logs / f"{role}.log").open("a") as out, (self.logs / f"{role}-error.log").open("a") as err:
                process = subprocess.Popen([str(executable), *map(str, arguments)], cwd=cwd or self.root,
                                           env=env, stdout=out, stderr=err, stdin=subprocess.DEVNULL,
                                           start_new_session=True)
            self.children.append(process)
            identity = wait_for(lambda: process_identity(process.pid), 2, f"Cannot identify new {role} process")
            record = {"pid": process.pid, "identity": identity, "owned": True}
            time.sleep(0.3)
            if process.poll() is not None:
                raise RuntimeError(f"{role} exited ({process.returncode}); see {self.logs}/{role}-error.log")
            print(f"Started {role} (PID {process.pid})")
        record["role"] = role
        self.state["processes"] = [r for r in self.state["processes"] if r["role"] != role] + [record]
        self.save()
        return record

    def stop(self):
        streams = hub_state() or []
        for record in sorted(self.state["processes"], key=lambda r: r["role"] == "hub"):
            if not live_record(record):
                continue
            if not record["owned"] or record["role"] == "bespoke":
                print(f"Leaving {record['role']} open (PID {record['pid']}); close Bespoke normally to preserve patches")
                continue
            try:
                os.kill(record["pid"], signal.SIGTERM)
            except ProcessLookupError:
                continue
            for child in self.children:
                if child.pid == record["pid"]:
                    try:
                        child.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        pass
            print(f"Requested stop: {record['role']} (PID {record['pid']})")
            if record["role"] == "rustel":
                wait_for(lambda: not live_record(record), 4, "Rustel has not stopped; shared memory was left intact")
                for session in streams:
                    if session["session"].startswith(f"rustel-{record['pid']}-") and session.get("lane_audio"):
                        unlink_stopped_mapping(session["lane_audio"]["mapping"], record["pid"])

    def status(self):
        for record in self.state["processes"]:
            print(f"{record['role']}: {'running' if live_record(record) else 'stopped'} / PID {record['pid']} / "
                  f"{'launcher-owned' if record['owned'] else 'reused'}")
        sessions = hub_state()
        if sessions is None:
            raise RuntimeError("Music hub unavailable")
        for session in sessions:
            print(f"{session['session']}: {len(session.get('lanes', []))} lanes")

    def start(self, args):
        score = Path(args.score).expanduser().resolve()
        if not score.is_file():
            raise RuntimeError(f"Score not found: {score}")
        # Permit ongoing source edits; startup must not reapply a vendor snapshot
        # or demand byte-for-byte agreement with it during development.
        required = ["rustel/crates/runtime/src/session/music.rs", "BespokeSynth/Source/RustelView.cpp",
                    "hearsay/Cargo.toml"]
        if any(not (self.root / path).is_file() for path in required):
            raise RuntimeError("Integration source is missing; run ./Setup.sh before starting")
        hub = self.folder / "target/debug/music-hub"
        rustel = self.root / "rustel/target/debug/rustel"
        if args.build or not hub.is_file():
            run(["cargo", "build", "--manifest-path", self.folder / "Cargo.toml", "--bin", "music-hub"], self.root)
        if args.build or not rustel.is_file():
            run(["cargo", "build", "--manifest-path", self.root / "rustel/Cargo.toml", "-p", "rustel",
                 "--no-default-features", "--features", "studio,music-link"], self.root)
        candidates = bespoke_candidates(self.root, sys.platform)
        bespoke = next((p for p in candidates if p.is_file()), None)
        if not args.no_bespoke and (args.build or bespoke is None):
            build = self.root / "BespokeSynth/ignore/music-build-posix"
            run(["cmake", "-S", self.root / "BespokeSynth", "-B", build, "-DCMAKE_BUILD_TYPE=Release",
                 "-DBESPOKE_ENABLE_PYTHON=ON"], self.root)
            run(["cmake", "--build", build, "--config", "Release", "--target", "BespokeSynth",
                 "--parallel", str(min(os.cpu_count() or 2, 4))], self.root)
            bespoke = next((p for p in candidates if p.is_file()), None)
            if bespoke is None:
                raise RuntimeError("Bespoke build completed but its executable was not found under music-build-posix")
        if hub_state() is None:
            self.ensure("hub", hub)
        wait_for(lambda: hub_state() is not None, 15, "Music hub did not become ready; check ports 9612–9614 and logs")
        env = dict(os.environ, RUSTEL_MUSIC_LINK="1", RUSTEL_LANE_AUDIO="1" if args.lane_audio else "0")
        arguments = ["play", str(score), "--watch", "--no-update-check", "--no-input"]
        if args.silent:
            arguments += ["--output", "silent"]
        player = self.ensure("rustel", rustel, arguments, env)
        wait_for(hub_state, 20, "Rustel is not publishing lanes; see music-link/logs/rustel-error.log")
        if args.lane_audio:
            wait_for(lambda: any(s["session"].startswith(f"rustel-{player['pid']}-") and s.get("lane_audio")
                                 for s in (hub_state() or [])), 8,
                     "No lane audio export from this Rustel. Rebuild the new source, or stop an existing process started without lane audio and relaunch.")
        if not args.no_bespoke:
            preferences = self.folder / "native-view-prefs-posix.json"
            if not preferences.exists():
                preferences.write_text(json.dumps({"layout": str(self.folder / "native-view-layout.json"),
                                                  "audio_input_device": "none", "audio_output_device": "auto"}, indent=2))
            self.ensure("bespoke", bespoke, [preferences], cwd=self.root / "BespokeSynth")
        if args.web:
            build_viewer(self.root, args.build)
            url = f"http://127.0.0.1:{args.web_port}/"
            try:
                with HTTP.open(url, timeout=1) as response:
                    served = "<title>Nightshade Viewer</title>" in response.read(1_000_000).decode(errors="replace")
                if not served:
                    raise RuntimeError(f"Port {args.web_port} is occupied by another application")
            except urllib.error.HTTPError as error:
                raise RuntimeError(f"Port {args.web_port} is occupied (HTTP {error.code}); choose --web-port") from error
            except urllib.error.URLError:
                self.ensure("web", sys.executable, ["-m", "http.server", str(args.web_port), "--bind", "127.0.0.1",
                                                   "--directory", str(self.root / "nightshade-viewer/dist")], discover=False)
                wait_for(lambda: page_ready(url), 10, "Viewer HTTP server did not become ready")
            if not args.no_open and not self.state["web_opened"]:
                if webbrowser.open(url + "?music=1"):
                    self.state["web_opened"] = True
                    self.save()
            print(f"Web viewer: {url}?music=1")
        print("Ready. " + ("Per-lane stereo PCM export enabled." if args.lane_audio else "Lane PCM export disabled."))
        print(f"Logs: {self.logs}")


def page_ready(url):
    try:
        with HTTP.open(url, timeout=1) as response:
            return response.status == 200
    except OSError:
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--score", default=str(ROOT / "music-link/demo.strudel"))
    parser.add_argument("--web", action="store_true")
    parser.add_argument("--web-port", type=int, default=18765)
    parser.add_argument("--silent", action="store_true")
    parser.add_argument("--build", action="store_true")
    parser.add_argument("--status", action="store_true")
    parser.add_argument("--stop", action="store_true")
    parser.add_argument("--no-open", action="store_true", help="Print the viewer URL without opening a browser")
    parser.add_argument("--no-bespoke", action="store_true", help="Run Rustel/hub and optional web viewer only")
    parser.add_argument("--lane-audio", action=argparse.BooleanOptionalAction, default=True,
                        help="Per-lane stereo audio export (default enabled; --no-lane-audio disables)")
    args = parser.parse_args()
    if sys.platform not in ("linux", "darwin"):
        parser.error("On Windows, use Start Music.cmd or music-link/start.ps1")
    if not 1024 <= args.web_port <= 65535:
        parser.error("--web-port must be between 1024 and 65535")
    if args.status and args.stop:
        parser.error("Choose either --status or --stop")
    try:
        with launcher_lock(ROOT / "music-link/launcher-posix.lock"):
            launcher = Launcher()
            if args.stop:
                launcher.stop()
            elif args.status:
                launcher.status()
            else:
                launcher.start(args)
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Music launcher: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
