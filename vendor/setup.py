"""Portable, checked vendor-patch setup; requires Python 3.9+ and Git."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent


def git(repository, *arguments, check=True):
    result = subprocess.run(["git", "-C", str(repository), *arguments],
                            capture_output=True, text=True, encoding="utf-8", errors="replace")
    if check and result.returncode:
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return result


def checked_path(root, relative):
    path = (root / relative).resolve()
    if not path.is_relative_to(root.resolve()):
        raise RuntimeError(f"Manifest path escapes workspace: {relative}")
    return path


def verify(path, expected):
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        raise RuntimeError(f"Vendor checksum mismatch: {path}")


def upgrade_files(root, project):
    """Replace only source that matches a known vendored revision or upstream."""
    planned = []
    repo = checked_path(root, project["name"])
    for file in project["files"]:
        relative = file["path"]
        target = checked_path(repo, relative)
        snapshot = checked_path(root / "vendor" / project["name"] / "files", relative)
        desired = None if file.get("deleted") else snapshot.read_bytes()
        current = target.read_bytes() if target.exists() else None
        normalize = lambda data: None if data is None else data.replace(b"\r\n", b"\n")
        if normalize(current) == normalize(desired):
            continue
        known = []
        upstream = subprocess.run(["git", "-C", str(repo), "show", f"{project['base']}:{relative}"], capture_output=True)
        if upstream.returncode == 0:
            known.append(upstream.stdout)
        else:
            known.append(None)
        history_path = f"vendor/{project['name']}/files/{relative}"
        history = git(root, "log", "--format=%H", "--", history_path, check=False)
        for commit in history.stdout.splitlines():
            previous = subprocess.run(["git", "-C", str(root), "show", f"{commit}:{history_path}"], capture_output=True)
            if previous.returncode == 0:
                known.append(previous.stdout)
        if normalize(current) not in [normalize(data) for data in known]:
            raise RuntimeError(f"{project['name']}/{relative}: conflicting local edits; nothing overwritten")
        planned.append((target, desired))
    return planned


def apply_workspace(root=ROOT, initialize=False, check_only=False, upgrade=False, ensure=False):
    # checked_path returns resolved targets. Keep their root in the same form,
    # including macOS /var aliases and Windows short-name temp directories.
    root = Path(root).resolve()
    manifest = json.loads((root / "vendor/manifest.json").read_text(encoding="utf-8"))
    if manifest["version"] != 1:
        raise RuntimeError("Unsupported vendor manifest version")
    manifest_hash = hashlib.sha256((root / "vendor/manifest.json").read_bytes()).hexdigest()
    marker = root / "music-link/vendor-applied.json"
    if ensure and marker.exists() and not check_only:
        if json.loads(marker.read_text()).get("manifest") == manifest_hash:
            return
    if initialize and not check_only:
        # Git fetches only the published pins; the integration comes from vendor/.
        subprocess.run(["git", "-C", str(root), "submodule", "update", "--init", "--recursive"], check=True)
    pending = []
    replacements = []
    for project in manifest["projects"]:
        repo = checked_path(root, project["name"])
        if git(repo, "rev-parse", "HEAD").stdout.strip() != project["base"]:
            raise RuntimeError(f"{project['name']} is not at its pinned upstream base; initialize its submodule first")
        if not project.get("patch_sha256"):
            continue
        folder = checked_path(root / "vendor", project["name"])
        patch = folder / "changes.patch"
        verify(patch, project["patch_sha256"])
        for file in project["files"]:
            if not file.get("deleted"):
                verify(checked_path(folder / "files", file["path"]), file["sha256"])
        if git(repo, "apply", "--reverse", "--check", str(patch), check=False).returncode == 0:
            print(f"{project['name']}: vendor changes already present")
            continue
        result = git(repo, "apply", "--check", str(patch), check=False)
        if result.returncode:
            if upgrade:
                replacements.extend(upgrade_files(root, project))
                continue
            raise RuntimeError(f"{project['name']}: conflicting local edits; no patches applied.\n{result.stderr}")
        pending.append((repo, patch))
    # No source is changed until every project passes preflight.
    if not check_only:
        for repo, patch in pending:
            git(repo, "apply", "--whitespace=nowarn", str(patch))
            print(f"{repo.name}: applied vendor changes")
        for target, contents in replacements:
            if contents is None:
                target.unlink(missing_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(contents)
            print(f"Updated vendored source: {target.relative_to(root)}")
        if marker.parent.exists():
            marker.write_text(json.dumps({"manifest": manifest_hash}) + "\n")
    print("Vendor source checks passed." if check_only else "Workspace source ready. Build/start instructions: music-link/README.md")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--initialize", action="store_true", help="Initialize upstream submodules before applying patches")
    parser.add_argument("--check", action="store_true", help="Read-only checksum and patch preflight; does not fetch or apply")
    parser.add_argument("--upgrade", action="store_true", help="Upgrade source matching an older vendored snapshot, preserving unknown edits")
    parser.add_argument("--ensure", action="store_true", help="Skip unchanged manifest after successful preparation (launcher use)")
    args = parser.parse_args()
    try:
        apply_workspace(initialize=args.initialize, check_only=args.check, upgrade=args.upgrade, ensure=args.ensure)
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Setup: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
