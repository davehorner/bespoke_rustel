# Upstream contribution drafts

Start an architecture discussion using the working demonstration, then split
changes by repository. These are copy-ready drafts for the contributor to review;
no issue, PR, email or message has been submitted. Upload the screenshots with the
post or attach this package so its relative image links have real destinations.

## Rustel discussion draft

**Title:** Optional live lane telemetry and audition controls for external tools

I have a working prototype that lets Bespoke and Nightshade observe the lanes of
a running Rustel session. It exposes accepted scheduler events, full score source
and source-context ranges, plus master audio analysis. Bespoke adds a lane source
node, transpose node and native viewer. Both viewers can ask Rustel to solo a
selected lane.

Rustel remains the sound engine. The integration observes existing scheduling,
uses an optional `music-link` feature and runtime opt-in, and sends network traffic
on a separate thread. Transpose and Solo act at the event boundary; queued notes
and effect tails can finish. It does not expose arbitrary pattern transformations.

Would this fit as an optional external observation/control interface? Before
proposing a merge, I would like feedback on using the typed observation built on existing scheduler trace
structures, the correct ownership of the shared message schema, and whether audition
controls belong at this boundary. The current shared crate uses sibling paths,
so this is an architecture proposal rather than a standalone merge-ready patch.

Attached: Stage, time spiral and Solo screenshots; architecture, demo and focused
validation evidence. The full repository checks and broader platform validation
remain outstanding. The prototype was developed with AI assistance.

### Proposed Rustel PR scope

**Title:** Add opt-in music telemetry and lane audition overlays

Attach bounded typed scheduler observations to drained events, preserve lane provenance when
the bridge is enabled, and publish session/lane/event state through the optional
bridge. Forward existing master analysis outside the audio callback. Validate
request session/generation/lane and acknowledge absolute transpose and shared
Solo changes. Keep normal behavior unchanged when the bridge is disabled.

| Area | Current files |
| --- | --- |
| Provenance | `crates/jsruntime/src/install/repl/lanes.rs` |
| Typed lane context and scheduler observation | `crates/core/src/context_keys.rs`, `crates/core/src/lib.rs`, `crates/scheduler/src/lib.rs` |
| Runtime hook and state | `crates/runtime/src/session.rs`, `session/scheduling.rs`, `session/music.rs` |
| Lane source | `crates/runtime/src/vst/lanes.rs` |
| CLI and Studio analysis | `crates/cli/src/live.rs`, `crates/studio/src/engine/ui_state.rs` |
| Feature wiring | CLI/runtime/Studio Cargo.toml files |

Focused tests cover source ranges, full lane source, transpose preservation and
solo filtering/restoration. Use EVIDENCE.md for completed checks and attach the
full upstream results only after actually running them.

## Bespoke discussion draft

**Title:** Native modules for observing and auditioning live Rustel lanes

I have implemented three native modules: `rustelpattern` selects a lane from a
running Rustel instance, `rusteltranspose` supplies an absolute semitone offset,
and `rustelview` visualizes its events and source on the patch canvas. The viewer
also includes audio-analysis modes and a Solo checkbox that asks Rustel to play
the selected lane.

The viewer uses Bespoke drawing and controls. It does not embed a browser or add a
Rust engine dependency to Bespoke. A shared JUCE background thread talks to a
loopback adapter; drawing and audio callbacks do not make network requests.

Would an integration like this belong in-tree, and should the external engine
client be a general interface or stay specific to Rustel? I can separate the
connection/source/transpose modules from the viewer for review. The prototype
currently requires the separate hub and modified Rustel; native screenshots and
the complete contribution checks should accompany a formal PR.

### Proposed Bespoke PR scope

**Title:** Add live Rustel lane nodes and a native pattern viewer

Add `RustelPattern`, `RustelLink`, `RustelView`, `RustelViewModes` and
`RustelViewAudio`; register the modules and sources in ModuleFactory/CMake.
Include a minimal example layout, tooltips and module documentation. Describe
camera gestures, source highlighting, audition semantics and master-tap limits.

Build evidence: standard Release `BespokeSynth.exe` compiled with the modules.
Protocol/browser live tests verify the same HTTP Solo path, but do not substitute
for native GUI regression coverage or a native screenshot. Check the project's
contributor rights/license notice before signing or submitting anything; this
package does not assert that the contributor has completed that review.

## Nightshade Viewer discussion draft

**Title:** Optional live music mode with event and audio-analysis views

I have a `?music=1` mode that subscribes to a running Rustel bridge. It adds a
bounded 3D Stage, session/lane selection, full source cards and Solo audition.
Eight additional views draw on a page canvas: Piano roll, Wordfall, time Spiral,
Pitch wheel, Scope, Spectrum, Vectorscope and onset Control curves.

The page canvas allows useful views without WebGPU and keeps them available if
the worker stops. GPU rendering pauses in flat views. Worker errors are visible;
a heartbeat watchdog falls back to Piano roll. Stress and forced-worker-stop
checks passed; the original user-reported freeze was not independently reproduced,
so I would not describe the recovery changes as a universal driver fix.

Would this be suitable as an opt-in mode or a separate example/application?
Attached are current screenshots, the schema/data-flow document and reproducible
demo. Dependency packaging and checks of ordinary model-viewer behavior still
need to be completed before an independent PR.

### Proposed Nightshade PR scope

**Title:** Add opt-in live music visualization and lane audition

Keep ordinary startup as the model viewer. Add the music subscription/panel,
canvas modes, worker scene, protocol messages and visible worker errors. Share
or independently test the radial geometry against the native Bespoke formulas.
Include the tested bounds and fallback behavior in documentation. Review this
separately from general engine or renderer changes.

## Hearsay example proposal

**Title:** Document a Rustel and Bespoke live music integration example

This prototype uses Hearsay topics to connect a Rust producer, a browser WebSocket
subscriber and a JUCE client through a small HTTP adapter. It demonstrates latest
snapshot publication, a separate analysis stream and acknowledged commands.
Would a small integration example or a link to the project be useful?

Hearsay's working tree is clean in this package's manifest. There is no demonstrated
Hearsay library defect here, and no library-change PR is proposed solely to host
the application. Keep a usage example distinct from any later narrowly reproduced
library issue.

## Suggested submission sequence

1. Share the demonstration and ask about extension points.
2. Agree on schema ownership, versioning and licensing; replace sibling paths
   with a reproducible dependency arrangement.
3. Submit focused Rustel and Bespoke changes with their own required checks.
4. Submit the optional Nightshade mode or separate example, following maintainer
   preference; offer Hearsay a small example if useful.
5. Review contributor/license requirements, preserve original attribution and
   avoid uploading runtime profiles, generated binaries or machine-specific prefs.

The shared `music-link` crate currently lacks an explicit package license and
license file. Decide and document its licensing before publication. Existing
repositories retain their respective licenses; a manifest of base commits is
not permission to relicense their code.
