"""Build a self-contained GitHub Pages viewer from prepared vendored source."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
VIEWER = ROOT / "nightshade-viewer"
OUTPUT = ROOT / "music-link/target/pages"


def run(*args):
    executable = shutil.which(args[0]) or args[0]
    env = dict(os.environ, RUSTUP_TOOLCHAIN=os.environ.get("RUSTUP_TOOLCHAIN", "stable"))
    env["NO_COLOR"] = "true"  # Trunk expects a boolean rather than NO_COLOR=1.
    subprocess.run([executable, *args[1:]], cwd=VIEWER, check=True, env=env)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--public-url", default="/bespoke_rustel/")
    parser.add_argument("--dev", action="store_true", help="Use the faster development build for local path checks")
    args = parser.parse_args()
    base = args.public_url
    if not base.startswith("/") or not base.endswith("/") or any(c in base for c in "?#\\") or ".." in base:
        parser.error("--public-url must be an absolute URL path with a trailing slash")
    profile = "debug" if args.dev else "release"
    flags = [] if args.dev else ["--release"]
    run("cargo", "build", "-p", "worker", "--target", "wasm32-unknown-unknown", *flags, "--locked")
    run("wasm-bindgen", "--target", "web", "--out-dir", "runtime", "--out-name", "engine",
        f"target/wasm32-unknown-unknown/{profile}/worker.wasm")
    run("npm", "run", "build:css")
    run("trunk", "build", *flags, "--public-url", base, "--dist", str(OUTPUT))
    # Keep the local viewer's query-string behavior; only the hosted artifact
    # defaults to music mode. Preserve other query parameters and fragments.
    index = OUTPUT / "index.html"
    html = index.read_text(encoding="utf-8")
    if "<head>" not in html:
        raise RuntimeError("Built index has no head element for the hosted music-mode bootstrap")
    bootstrap = ('<script>const u=new URL(location.href);'
                 'if(!u.searchParams.has("music")){u.searchParams.set("music","1");'
                 'location.replace(u.href);}</script>')
    html = html.replace("<head>", "<head>" + bootstrap, 1)
    html = html.replace("<title>Nightshade Viewer</title>", "<title>bespoke_rustel</title>")
    index.write_text(html, encoding="utf-8")
    (OUTPUT / ".nojekyll").touch()
    print(f"Pages artifact ready: {index.parent} (URL prefix {base})")


if __name__ == "__main__":
    main()
