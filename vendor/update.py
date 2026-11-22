"""Refresh parent-tracked source overlays from the current submodule worktrees."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import tempfile

VENDOR = Path(__file__).resolve().parent
ROOT = VENDOR.parent


def git(repo, *args, env=None):
    result = subprocess.run(["git", "-C", str(ROOT / repo), *args], capture_output=True, env=env)
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
    return result.stdout


def main():
    manifest = json.loads((VENDOR / "manifest.json").read_text(encoding="utf-8"))
    for project in manifest["projects"]:
        repo, base = project["name"], project["base"]
        paths = set(filter(None, git(repo, "diff", "--name-only", "-z", base).decode().split("\0")))
        paths.update(filter(None, git(repo, "ls-files", "--others", "--exclude-standard", "-z").decode().split("\0")))
        destination = VENDOR / repo
        destination.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="music-link-vendor-index-") as temporary:
            env = dict(os.environ, GIT_INDEX_FILE=str(Path(temporary) / "index"))
            git(repo, "read-tree", base, env=env)
            if paths:
                git(repo, "add", "--all", "--", *sorted(paths), env=env)
            patch = git(repo, "diff", "--cached", "--binary", "--full-index", "--no-ext-diff", base, env=env)
            project["result_tree"] = git(repo, "write-tree", env=env).decode().strip()
        previous = {file["path"] for file in project["files"] if not file.get("deleted")}
        files = []
        for relative in sorted(paths):
            source = ROOT / repo / relative
            if source.is_file():
                data = source.read_bytes()
                if len(data) > 2_000_000 or source.suffix.lower() in {".exe", ".dll", ".pdb", ".wasm", ".zip"}:
                    raise ValueError(f"Refusing generated/large vendor file: {repo}/{relative}")
                target = destination / "files" / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
                files.append({"path": relative, "sha256": hashlib.sha256(data).hexdigest()})
            else:
                files.append({"path": relative, "deleted": True})
        # Delete only obsolete copies listed by our previous vendor manifest.
        current = {file["path"] for file in files if not file.get("deleted")}
        for relative in previous - current:
            obsolete = (destination / "files" / relative).resolve()
            if not obsolete.is_relative_to((destination / "files").resolve()):
                raise ValueError("Vendor path escaped its source-copy directory")
            obsolete.unlink(missing_ok=True)
        project["files"] = files
        if patch:
            (destination / "changes.patch").write_bytes(patch)
            project["patch_sha256"] = hashlib.sha256(patch).hexdigest()
        else:
            (destination / "changes.patch").unlink(missing_ok=True)
            project.pop("patch_sha256", None)
        print(f"{repo}: {len(files)} vendored source files")
    (VENDOR / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
