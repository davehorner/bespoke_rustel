# Bespoke / Rustel music link workspace

Live Rustel pattern observation, controls and visualizations in Bespoke Synth
and Nightshade. Start with [the integration guide](music-link/README.md).

## Repository layout

The parent repository tracks `music-link/`, launch scripts and workspace docs.
`BespokeSynth`, `rustel`, `nightshade-viewer` and `hearsay` are Git submodules;
their source and history live in their own repositories. O2 is not currently
part of this workspace.

Build outputs, dependency caches, downloaded tools, runtime logs, local browser
profiles and generated ZIPs are ignored. Selected contribution screenshots are
tracked intentionally. Nothing on disk is deleted by these ignore rules.

## Local commits and publishing

The initial setup preserves integration changes in local submodule commits on
`music-link-integration` branches. The original upstream remotes remain intact.
No repositories, forks or commits are published automatically.

**Before sharing a recursive clone:** push each modified submodule branch to a
repository you control. Update that submodule's URL in `.gitmodules` to its
published fork, run `git submodule sync`, then commit `.gitmodules` in the parent.
The current upstream URLs do not contain the local integration commits until
those changes are published there. The parent also needs a remote you choose.

After publishing, a fresh checkout uses:

```powershell
git clone --recurse-submodules <your-parent-repository-url>
```

Build prerequisites and commands are in `music-link/README.md`. Bespoke has its
own nested dependency submodules, so recursive initialization is required.

## Saving subsequent changes

Commit in each changed submodule first, then record its new commit in the parent:

```powershell
git -C rustel add <changed-source-files>
git -C rustel commit -m "Describe the Rustel change"
git add rustel music-link
git diff --cached --stat
git commit -m "Update music link integration"
```

Use the same process for BespokeSynth or nightshade-viewer. A parent commit alone
does not save uncommitted source edits inside a submodule. Push submodule commits
before pushing the parent commit that refers to them.

Check workspace changes with `git status` and, to include submodule details,
`git submodule foreach --recursive git status --short`. Root ignore rules do not
apply inside submodules; keep each project's own ignore rules up to date.
