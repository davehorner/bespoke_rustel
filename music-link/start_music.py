"""macOS/Linux workspace launcher (Python 3.9+). Windows uses start_music.ps1."""
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
from build_inputs import needs_build

ROOT = Path(__file__).resolve().parent.parent
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))
# Rustel can spend 60 seconds preloading samples before its first publication.
RUSTEL_LANE_TIMEOUT = 90
RUSTEL_PROFILE = "local" if sys.platform == "darwin" else "dev"
RUSTEL_OUTPUT = "local" if RUSTEL_PROFILE == "local" else "debug"


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
            result = subprocess.run(["ps", "-p", str(pid), "-o", "state=", "-o", "lstart=", "-o", "comm="],
                                    capture_output=True, text=True, timeout=2)
            # State precedes the five lstart fields and the executable path.
            parts = result.stdout.strip().split(None, 6)
            if result.returncode or len(parts) != 7 or parts[0].startswith("Z"):
                return None
            return {"started": " ".join(parts[1:6]), "executable": str(Path(parts[6]).resolve())}
    except (OSError, ValueError, IndexError, subprocess.TimeoutExpired):
        pass
    return None


def live_record(record):
    return process_identity(record["pid"]) == record["identity"]


def quit_bespoke(record):
    # Ask the exact running app to quit through AppKit's normal quit path.
    script = """
    ObjC.import('AppKit');
    function run(argv) {
        const app = $.NSRunningApplication.runningApplicationWithProcessIdentifier(Number(argv[0]));
        return !app.isNil() && ObjC.unwrap(app.executableURL.path) === argv[1] && app.terminate;
    }
    """

    def request():
        if not live_record(record):
            return True
        result = subprocess.run(["/usr/bin/osascript", "-l", "JavaScript", "-e", script,
                                 str(record["pid"]), record["identity"]["executable"]],
                                capture_output=True, text=True, timeout=3)
        if result.returncode:
            raise RuntimeError(f"Could not quit Bespoke. {result.stderr.strip()}")
        return result.stdout.strip() == "true"

    # A newly spawned process may take a moment to register with AppKit.
    wait_for(request, 5, f"Bespoke (PID {record['pid']}) did not accept a normal quit request")


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
    if not rebuild and not needs_build("nightshade-viewer", viewer / "dist/index.html", root):
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
            time.sleep(0.3)
            if process.poll() is not None:
                raise RuntimeError(f"{role} exited ({process.returncode}); see {self.logs}/{role}-error.log")
            # Capture after the startup check, not during the child/interpreter's
            # early exec transition (notably macOS framework Python launchers).
            identity = wait_for(lambda: process_identity(process.pid), 2, f"Cannot identify new {role} process")
            record = {"pid": process.pid, "identity": identity, "owned": True}
            print(f"Started {role} (PID {process.pid})")
        record["role"] = role
        self.state["processes"] = [r for r in self.state["processes"] if r["role"] != role] + [record]
        self.save()
        return record

    def stop(self):
        streams = hub_state() or []
        errors = []
        # Keep the hub available while clients shut down.
        order = {"bespoke": 0, "hub": 2}
        for record in sorted(self.state["processes"], key=lambda r: order.get(r["role"], 1)):
            if not live_record(record):
                continue
            role, pid = record["role"], record["pid"]
            if not record["owned"]:
                print(f"Leaving reused {role} open (PID {pid})")
                continue
            if role == "bespoke" and sys.platform != "darwin":
                print(f"Leaving bespoke open (PID {pid}); close its window to quit")
                continue
            try:
                if role == "bespoke":
                    quit_bespoke(record)
                else:
                    try:
                        os.kill(pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
                print(f"Requested stop: {role} (PID {pid})", flush=True)
                # Rustel allows up to eight seconds for the audio tail alone.
                failure = (f"Rustel (PID {pid}) is still running; its shared memory was left intact"
                           if role == "rustel" else f"{role} (PID {pid}) is still running")
                child = next((child for child in self.children if child.pid == pid), None)
                wait_for(lambda: child.poll() is not None if child is not None else not live_record(record),
                         15 if role == "rustel" else 5, failure)
                if role == "rustel":
                    for session in streams:
                        if session["session"].startswith(f"rustel-{pid}-") and session.get("lane_audio"):
                            unlink_stopped_mapping(session["lane_audio"]["mapping"], pid)
            except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
                errors.append(str(error))
        if errors:
            raise RuntimeError("\n".join(errors))

    def status(self):
        for record in self.state["processes"]:
            print(f"{record['role']}: {'running' if live_record(record) else 'stopped'} / PID {record['pid']} / "
                  f"{'launcher-owned' if record['owned'] else 'reused'}")
        sessions = hub_state()
        if sessions is None:
            raise RuntimeError("Music hub unavailable")
        for session in sessions:
            print(f"{session['session']}: {len(session.get('lanes', []))} lanes")

    def wait_for_rustel_lanes(self, player):
        prefix = f"rustel-{player['pid']}-"
        log = self.logs / "rustel-error.log"

        def ready():
            if not live_record(player):
                raise RuntimeError(f"Rustel exited before publishing lanes; see {log}")
            return any(s["session"].startswith(prefix) and s.get("lanes") for s in (hub_state() or []))

        if not ready():
            print("Waiting for Rustel lanes. Cold sample downloads can delay startup.", flush=True)
            wait_for(ready, RUSTEL_LANE_TIMEOUT,
                     f"Rustel did not publish lanes within {RUSTEL_LANE_TIMEOUT} seconds; see {log}")

    def start(self, args):
        score = Path(args.score).expanduser().resolve()
        if not score.is_file():
            raise RuntimeError(f"Score not found: {score}")
        subprocess.run([sys.executable, str(self.root / "vendor/setup.py"), "--upgrade", "--ensure"], check=True)
        # Permit ongoing source edits; startup must not reapply a vendor snapshot
        # or demand byte-for-byte agreement with it during development.
        required = ["rustel/crates/runtime/src/session/music.rs", "BespokeSynth/Source/RustelView.cpp",
                    "hearsay/Cargo.toml"]
        if any(not (self.root / path).is_file() for path in required):
            raise RuntimeError("Integration source is missing; run ./Setup.sh before starting")
        hub = self.folder / "target/debug/music-hub"
        rustel = self.root / "rustel/target" / RUSTEL_OUTPUT / "rustel"
        if args.build or needs_build("music-link", hub, self.root):
            if hub.is_file() and find_existing(hub):
                raise RuntimeError("Hub source was updated; stop the running hub before rebuilding")
            run(["cargo", "build", "--manifest-path", self.folder / "Cargo.toml", "--bin", "music-hub"], self.root)
        if args.build or needs_build("rustel", rustel, self.root):
            if rustel.is_file() and find_existing(rustel):
                raise RuntimeError("Rustel source was updated; close Rustel before rebuilding")
            run(["cargo", "build", "--manifest-path", self.root / "rustel/Cargo.toml", "-p", "rustel",
                 "--profile", RUSTEL_PROFILE, "--no-default-features", "--features", "studio,music-link"], self.root)
        candidates = bespoke_candidates(self.root, sys.platform)
        bespoke = next((p for p in candidates if p.is_file()), None)
        if not args.no_bespoke and (args.build or bespoke is None or needs_build("BespokeSynth", bespoke, self.root)):
            if bespoke and find_existing(bespoke):
                raise RuntimeError("Bespoke source was updated. Save and close Bespoke, then rerun start_music.sh to rebuild; the old executable will not be reused")
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
        self.wait_for_rustel_lanes(player)
        if args.lane_audio:
            wait_for(lambda: any(s["session"].startswith(f"rustel-{player['pid']}-") and s.get("lane_audio")
                                 for s in (hub_state() or [])), 8,
                     "No lane audio export from this Rustel. Rebuild the new source, or stop an existing process started without lane audio and relaunch.")
        if not args.no_bespoke:
            preferences = self.folder / "native-view-prefs-posix.json"
            demo_preferences = (json.loads(preferences.read_text()) if preferences.exists() else
                                {"layout": str(self.folder / "native-view-layout.json"),
                                 "audio_input_device": "none", "audio_output_device": "auto"})
            # Demo purposes only: open the music patch without the welcome overlay.
            # Preserve other demo settings; do not modify normal Bespoke preferences.
            demo_preferences["show_welcome_screen"] = False
            preferences.write_text(json.dumps(demo_preferences, indent=2) + "\n")
            self.ensure("bespoke", bespoke, [preferences], cwd=self.root / "BespokeSynth")
        if args.web:
            self.start_web(args)
        print("Ready. " + ("Per-lane stereo PCM export enabled." if args.lane_audio else "Lane PCM export disabled."))
        print(f"Logs: {self.logs}")

    def start_web(self, args):
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
        print(f"Nightshade viewer: {url}?music=1", flush=True)
        if not args.no_open:
            try:
                if not webbrowser.open(url + "?music=1"):
                    print("Browser could not open; use the Nightshade URL printed above.")
            except (OSError, webbrowser.Error) as error:
                print(f"Browser could not open; use the Nightshade URL printed above. {error}")


def page_ready(url):
    try:
        with HTTP.open(url, timeout=1) as response:
            return response.status == 200
    except OSError:
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--score", default=str(ROOT / "music-link/demo.strudel"))
    parser.add_argument("--web", action=argparse.BooleanOptionalAction, default=True,
                        help="Start Nightshade and open its URL (default; --no-web disables)")
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
        parser.error("On Windows, use start_music.cmd or music-link/start_music.ps1")
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
