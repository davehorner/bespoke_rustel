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


def apply_workspace(root=ROOT, initialize=False, check_only=False):
    manifest = json.loads((root / "vendor/manifest.json").read_text(encoding="utf-8"))
    if manifest["version"] != 1:
        raise RuntimeError("Unsupported vendor manifest version")
    if initialize and not check_only:
        # Git fetches only the published pins; the integration comes from vendor/.
        subprocess.run(["git", "-C", str(root), "submodule", "update", "--init", "--recursive"], check=True)
    pending = []
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
            raise RuntimeError(f"{project['name']}: conflicting local edits; no patches applied.\n{result.stderr}")
        pending.append((repo, patch))
    # No source is changed until every project passes preflight.
    if not check_only:
        for repo, patch in pending:
            git(repo, "apply", "--whitespace=nowarn", str(patch))
            print(f"{repo.name}: applied vendor changes")
    print("Vendor source checks passed." if check_only else "Workspace source ready. Build/start instructions: music-link/README.md")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--initialize", action="store_true", help="Initialize upstream submodules before applying patches")
    parser.add_argument("--check", action="store_true", help="Read-only checksum and patch preflight; does not fetch or apply")
    args = parser.parse_args()
    try:
        apply_workspace(initialize=args.initialize, check_only=args.check)
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Setup: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
