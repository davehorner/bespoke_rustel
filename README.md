# Bespoke / Rustel music link workspace

Live Rustel pattern observation, controls, per-lane stereo audio and visualizations
in Bespoke Synth and Nightshade. See [the integration guide](music-link/README.md).

## Clone and prepare

```powershell
git clone --recurse-submodules https://github.com/davehorner/bespoke_rustel.git
cd bespoke_rustel
.\Setup.ps1
```

PowerShell 7 and Git are required for setup. Build prerequisites and commands are
in `music-link/README.md`. Then use `Start Music.cmd` to launch with lane audio.
The launcher uses your built executables; setup prepares source and does not build.

## Where the code lives

The parent repository tracks `music-link/`, launch scripts, documentation and
**all integration changes under [`vendor/`](vendor/README.md)**. The `files/`
folders contain readable source copies, while patches reproduce the changes in
the normal project directories. This includes the Rustel runtime bridge,
Bespoke native nodes and Nightshade viewers, including `rustelaudio`.

`BespokeSynth`, `rustel`, `nightshade-viewer` and `hearsay` remain submodules pinned
to published upstream commits. Setup initializes their dependencies and applies
the parent-tracked patches. Custom forks or unpublished submodule commits are
not needed. The source inside those submodules will show diffs after setup;
that is expected. O2 is not currently part of this workspace.

Build outputs, dependency caches, downloaded tools, runtime logs, browser profiles
and generated ZIPs are ignored. Selected contribution screenshots are tracked.
The vendor directory contains changed source, not complete upstream histories
or sample libraries. Existing local files are preserved.

## Saving and pushing changes

Edit source in the normal project directories. Refresh its parent-tracked copy
before committing or pushing:

```powershell
python vendor/update.py
git add -A
git diff --cached --stat
git commit -m "Update music link integration"
git push origin main
```

Python 3 is needed for the refresh command, but not for applying existing vendor
patches. Changes inside submodules are not saved by a parent commit until they
have been refreshed into `vendor/`. Source in `music-link/` is tracked directly.
Do not move the parent submodule pointers to unpublished local commits; the
vendored patches are based on the published pins in `vendor/manifest.json`.

Setup is idempotent: already-applied patches are skipped. If local edits conflict,
it stops instead of overwriting them. Existing local integration commits remain
on their original `music-link-integration` branches as backups; active local
`vendor-worktree` branches point at the published bases with the integration
visible as working-tree changes.
