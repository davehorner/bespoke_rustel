# Vendored integration changes

This directory makes the integration code part of the **parent repository**.
Each project's `files/` contains readable copies of every changed or new source
file, including the uncommitted `rustelaudio` implementation. `changes.patch`
reproduces those edits on the published upstream base in `manifest.json`.
The copies are for review; builds use the normal project directories.

We retain upstream projects as submodules to avoid importing their full history,
sample libraries, bundled assets and build products into this repository.
Submodule pointers now refer to published upstream bases, so no unpublished
custom submodule commits are needed to reconstruct the workspace.

After cloning, run `../Setup.ps1`, which initializes nested dependencies and
applies these patches. It checks SHA-256 checksums and preflights every patch;
already-applied patches are skipped, while conflicting edits stop the operation.
It does not discard local edits or create commits. Patched submodules are
expected to show working-tree diffs afterward.

When developing, edit files in the normal project directories, then run:

```powershell
python vendor/update.py
git add vendor music-link "Start Music.cmd"
git diff --cached --stat
git commit -m "Update vendored music link integration"
git push
```

Keep the pinned upstream bases unchanged unless deliberately upgrading a project.
The manifest also records the original local integration commits as backup
references; they are not required by setup. Upstream license files are copied
beside each project's overlay, and source copyright notices remain intact.
