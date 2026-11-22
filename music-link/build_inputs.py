"""Detect binaries that predate their integration source inputs."""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def needs_build(project, artifact, root=ROOT):
    artifact = Path(artifact)
    if not artifact.is_file():
        return True
    manifest = json.loads((root / "vendor/manifest.json").read_text())
    entry = next((p for p in manifest["projects"] if p["name"] == project), None)
    inputs = [root / project / f["path"] for f in entry["files"] if not f.get("deleted")] if entry else []
    if project in ("music-link", "rustel", "nightshade-viewer"):
        inputs += list((root / "music-link/src").glob("*.rs"))
        inputs += [root / "music-link/Cargo.toml", root / "music-link/Cargo.lock"]
    return any(p.is_file() and p.stat().st_mtime_ns > artifact.stat().st_mtime_ns for p in inputs)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("project")
    parser.add_argument("artifact")
    args = parser.parse_args()
    print("true" if needs_build(args.project, args.artifact) else "false")
