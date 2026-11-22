# Verification evidence and limits

## Source identity

The full base commit hashes and changed/untracked source inventory are recorded
in [manifest.json](manifest.json). These are base revisions beneath a dirty
prototype. They are not release tags or published PR commits. Hearsay's working
tree is clean. The schema/hub is a sibling crate outside those upstream repos.

## Completed focused verification

| Check | Observed result |
| --- | --- |
| Shared protocol library | 3 tests passed: bounded/finite audio, transpose addressing and Solo generation/legacy compatibility. |
| Rustel music runtime | 4 tests passed: full lane source, transpose preservation, read-only observation/user-control preservation, Solo filtering/restoration. |
| Scheduler | 31 tests passed, including typed observation/editor queue independence and bounds with both consumers enabled. |
| Native Bespoke | Standard Release executable built successfully with all native view modules and Solo. |
| Rustel CLI/Studio | Built with `--no-default-features --features studio,music-link`. |
| Browser feature check | Worker and page checked for wasm32 with the agent feature. |
| Browser bundle | Worker, bindings, CSS and page built successfully. |
| Radial geometry | 2 standalone tests passed for the native-equivalent time spiral and octave-separated pitch wheel. |
| Live Solo | HTTP and real browser checkbox tests passed for enable, lane migration and release; all visual lanes and transpose values were preserved. |
| Browser view regression | All eight added views drew; Stage resumed after flat views; forced worker termination selected fallback; no-WebGPU music mode worked. |
| Dense synthetic score | 64 lanes and 2,048 events rendered during a 15-second browser test at roughly 70 FPS on this machine. This is a bounded stress observation, not a general performance benchmark. |
| Screenshot capture | Ten current 1600×1000 browser screenshots; zero page exceptions recorded during capture. Solo was released afterward. |

The last Solo live test restored normal playback. Screenshot source, session,
time and demo are recorded in the capture script/demo and image manifest. The
images show the final Solo control, not the removed source-trace buttons.

## Not established by these checks

- The original reported 3D freeze was not reproduced; hardening and recovery were
  tested, rather than a verified root-cause fix for that original failure.
- Native GUI automation and native screenshots are not complete. The Windows
  helper failed to initialize; the isolated built-in screenshot attempt did not
  yield a PNG. Browser screenshots do not verify native pixel layout.
- The full Rustel CONTRIBUTING matrix, all platform builds, full Clippy, Bespoke
  formatting/module documentation and ordinary model-viewer regression coverage
  have not all been completed for an upstream submission.
- Current local paths do not establish standalone upstream build reproducibility.
- Scope/Spectrum/Vectorscope use a master analysis feed, not isolated lane taps.
- Control curves sample onsets; Solo/transpose affect newly scheduled events.

## Re-run artifacts

`music-link/check-solo.mjs` and `music-link/check-nightshade.mjs` contain the live
functional checks. This package's `capture-browser.mjs` captures the gallery from
an isolated browser target. `manifest.json` hashes every included PNG, so reviewers
can distinguish later recaptures. Runtime/browser profiles and logs are excluded.

## Typed telemetry refactor checks

The refactor additionally passed all 325 core tests and all 75 JS runtime tests.
An isolated bridge-enabled integration test passed with a Unicode source prefix,
multiple lanes, stacked voices, signal controls and an `all` transform. Ordinary
control objects contained neither bridge metadata key; independent editor traces
retained exact source ranges and both lane identities. The core test checks that
host tagging preserves values, locations, scale context and live slider bindings.
The runtime also checked successfully with the bridge feature disabled.

The existing gallery predates this internal refactor; it remains representative
of the unchanged viewer UI and external message schema. These focused checks do
not replace the complete upstream contribution matrix.

Post-refactor live verification passed through the existing clients: all nine
lanes, 91 events with bounded source ranges in the sampled snapshot, resolved
pan controls and a +7 transposed event were observed. The transpose setting was
restored afterward. HTTP and browser Solo enable/migrate/release checks passed
again, preserving visualization and transpose values. The standard Rustel binary
was rebuilt and the demo restarted with the typed observation path.
