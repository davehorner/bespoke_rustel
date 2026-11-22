# Live Rustel, Bespoke and Nightshade

This first implementation discovers the `$:` lanes of an independently running
Rustel session, lets a Bespoke node transpose a selected lane, and displays
source/output events in Nightshade Viewer. Rustel continues rendering the sound.
Playback stays in Rustel; telemetry includes audio-analysis frames. No second
pattern evaluation is performed.

See [the upstream contribution package](contribution/README.md) for screenshots,
architecture, reproduction steps and discussion/PR drafts.

## Start

Double-click `Start Music.cmd` at the workspace root, or run:

```powershell
./music-link/start.ps1
```

The launcher builds missing native components, starts or reuses the hub, watched
demo player and native Bespoke viewer, and waits for live lanes before reporting
ready. Run it again without duplicating those apps. It does not close existing
patches or replace an existing Rustel session's score.

Options: `-Status` checks connections; `-Web` also serves/opens Nightshade;
`-Score path.strudel` chooses the score for a new player; `-Silent` starts a new
player without audio hardware; `-Build` explicitly rebuilds components (close the
affected apps first). Build tools and platform dependencies must be installed.
Logs are under `music-link/logs`.

`-Stop` stops only processes this launcher started, verifying their PID, start
time and executable path. Reused apps are left running. Bespoke receives a normal
close request so it can handle unsaved work. Multiple simultaneous launcher runs
are prevented by a file lock.

### Manual startup

Run commands from `R:\w\bespoke_rustel`, in separate PowerShell terminals.

1. Start the shared Hearsay broker and JUCE adapter:

   ```powershell
   cargo run --manifest-path music-link/Cargo.toml --bin music-hub
   ```

2. Build/run Rustel with the optional bridge. The environment setting must be
   present before evaluating the score:

   ```powershell
   $env:RUSTEL_MUSIC_LINK='1'
   cargo run --manifest-path rustel/Cargo.toml -p rustel --no-default-features --features studio,music-link -- studio
   ```

   Evaluate a score such as:

   ```js
   $: note("c3 e3 g3 b3").s("sawtooth").gain(0.15)
   $: s("bd*4")
   ```

   For a 30-second hardware-free probe (events, no sound):

   ```powershell
   cargo run --manifest-path rustel/Cargo.toml -p rustel-runtime --no-default-features --features music-link --example music_link
   ```

3. Build Bespoke normally after initializing its submodules:

   ```powershell
   cmake -S BespokeSynth -B BespokeSynth/ignore/music-build -A x64
   cmake --build BespokeSynth/ignore/music-build --config Release --parallel 4
   ```

   Spawn `rustelpattern` and `rusteltranspose`. Select a live lane in
   `rustelpattern`, connect its special cable to `rusteltranspose`, then turn
   `semitones`. Disconnecting the cable sends a zero offset. One transpose target
   per source is supported. Use one source node per lane to avoid competing
   commands. After restarting Rustel, select the new session's lane explicitly.
   Patch saves retain the selected binding when reconnecting to the same running
   session; changing selection or deleting the source attempts to clear its offset.

4. Build/serve Nightshade Viewer using its existing instructions:

   ```powershell
   Set-Location nightshade-viewer
   just run-web
   ```

   Open `http://127.0.0.1:8080/?music=1`. Music mode is opt-in; the ordinary model
   viewer is unchanged. Select a session in the music panel if several exist.
   Each lane has its own color. Thin traces show source pitch; thicker glowing
   blocks show output pitch and rise at each onset. Lane rings pulse as notes
   play, with a scrolling beat grid beneath them. Time moves along X, pitch along
   Y, and lanes along Z. The luminous crossbar is the playhead. Drag/scroll to
   orbit/zoom; use the HUD for automatic orbit, lane selection and Solo.

   The project's pinned wasm-bindgen CLI is 0.2.115. If the global tool differs,
   install it locally with `cargo install wasm-bindgen-cli --version 0.2.115
   --root ../music-link/tools --locked` and prepend that directory's `bin` to PATH.
   From the workspace root, `./music-link/build-viewer.ps1` rebuilds the worker,
   bindings, CSS and web bundle with the locally installed tool. The existing
   `nightshade-viewer/dist` folder can also be served by a static HTTP server.

## Boundaries

### Native Bespoke viewer

Spawn `rustelview` in the updated Bespoke executable for a native, resizable
perspective view directly on the patch canvas. It shares the existing connection
thread with `rustelpattern`; the music hub and bridged Rustel instance still need
to be running. Nightshade Viewer and the browser are not required.

The node discovers running sessions, displays all lanes or a selected lane, and
provides yaw, tilt, zoom, orbit and Solo controls. It draws
depth-sorted note blocks, colored lanes, onset rings and a moving beat grid with
Bespoke's own vector renderer. Its camera, size, selected session and lane are
saved in Bespoke patches. Rendering is capped at 1024 events and 64 lanes.
The scrollable panel below the note field shows each lane's complete source,
including multiline method chains. Select one lane to focus its code, or scroll
over the source panel to read all lanes. The browser viewer offers expandable
source cards. These require the updated Rustel bridge and music hub; older
publishers remain compatible but report source as unavailable.

Mouse controls inside the note field: left-drag orbits; right-drag or Shift +
left-drag pans; wheel/trackpad scroll zooms toward the cursor. Scrolling over code
scrolls its source panel instead. Use `reset` to restore framing. Camera pan is
saved with the patch, and existing patches retain their old camera settings.

The `view` dropdown selects Stage, Piano Roll, Wordfall, Spiral or Pitch Wheel.
Each module saves its own view, so several nodes can display one session in
different ways. The four flat views use drag-to-pan and wheel-to-zoom. They show
pitch/time events; audio waveform and spectrum modes use an analysis feed
from the updated Rustel player.

Scope (Master), Spectrum (Master) and Vectorscope (Master) use a separate 30 Hz
latest-frame analysis feed. They show Rustel's existing master tap **before master
gain and limiting**, not isolated lane audio. The scope uses rising-edge alignment;
the spectrum has logarithmic frequency bars, smoothing and decaying peak markers;
the vectorscope plots stereo mid/side with a correlation indicator. `display gain`
changes the picture only. Missing/stale audio shows a waiting state instead of
pretending that an old waveform is still live. Spectrogram history and individual
lane audio taps are not included yet.

The hub exposes current analysis at `http://127.0.0.1:9614/analysis`, fed by the
`music/audio` Hearsay topic. Frames include session, generation, sequence, stream,
epoch, sample rate and device-frame timestamp. FFT work runs on Rustel's producer
thread; encoding/networking runs on the bridge thread. No network or FFT work is
added to the audio callback.

The executable is
`BespokeSynth/ignore/music-build/Source/BespokeSynth_artefacts/Release/BespokeSynth.exe`.
Close the running Bespoke instance before rebuilding it.
Run `./music-link/run-native-view.ps1` to open a separate instance with the example
layout (`music-link/native-view-layout.json`). It uses its own generated
preferences file in `music-link`, leaving the original instance and preferences
alone. The layout contains the native view and the existing control nodes.

### Bridge behavior

- TCP broker: `127.0.0.1:9612`; browser WebSocket: `9613`; Bespoke HTTP: `9614`.
- Runtime feature `music-link` is optional. Without `RUSTEL_MUSIC_LINK=1`, it opens
  no connection. Normal builds do not acquire the Hearsay dependency.
- Stable lane numbers come from Rustel's existing committed lane table. Labels,
  source line numbers and current offsets accompany each snapshot. Lane numbers
  are scoped to the running Session; they are not cross-restart identifiers.
- Commands include session, score generation and request ID. The Rustel worker
  validates the target and acknowledges application. The HTTP adapter rejects
  stale/missing targets, waits for an acknowledgement, and reports timeouts.
- Transpose changes `note` (numeric or note-name) or explicit numeric `freq` at
  the producer's event boundary. Unpitched samples are unchanged. Scale-degree
  `n` patterns and chord lists are not transposed by this version.
- Event provenance is collected from labelled lanes (`$:` or named labels).
  Use a label for each pattern you want to visualize and control.
- Changes affect newly scheduled events. Existing lookahead plays through, so
  response latency is approximately Rustel's scheduling horizon. This is not
  an arbitrary pattern-transform graph: `slow`, `rev`, `every`, stacking and
  branch playback require a later pre-query graph API.
- The view shows the existing scheduler's lookahead, not an independently
  queried two-cycle prediction. Events are capped at 2048 per snapshot and the
  renderer pools at most 128 blocks per displayed lane (up to 64 lanes). Visual time is extrapolated locally and
  is approximate, rather than synchronized to an audio device sample clock.
- Snapshots replace earlier state, allowing late subscribers to catch up.
  They expire from the HTTP registry after three seconds without producer
  updates. The renderer hides stale event blocks after three seconds.
- Offsets survive ordinary code edits while the lane retains its ID. They are
  in-memory and reset when Rustel exits; Bespoke code never rewrites the score.
- This local trusted-process protocol has no remote authentication. Keep the
  listeners bound to loopback.

## Verification

```powershell
cargo test --manifest-path music-link/Cargo.toml
cargo test --manifest-path rustel/Cargo.toml -p rustel-runtime --no-default-features --features music-link session::music::tests --lib
cargo check --manifest-path nightshade-viewer/Cargo.toml -p worker -p nightshade-viewer --target wasm32-unknown-unknown --features agent
```

After building `music-hub` and the `music_link` example, `./music-link/smoke.ps1`
launches both, waits for two real Rustel lanes, sends +7, verifies modified
events and checks stale-generation rejection. It stops only its own processes.

### Source highlighting and control curves

The native viewer highlights sounding events' exact source tokens in the evaluated
full document. Ranges use UTF-8 byte offsets from Rustel's existing Hap context;
selecting a lane limits which events highlight the code. Wrapped lines and source
scrolling retain the original offsets. Helpers elsewhere in the document can also
highlight when their source context survives pattern composition.

Choose **control curves** in the view dropdown for per-lane gain, pan, cutoff and
resonance. Dots are values resolved at scheduled note onsets; step lines connect
those samples. Cutoff uses a logarithmic vertical scale. These are event control
samples, not continuous LFO queries or per-sample audio measurements. Missing
controls remain absent. Existing audio views continue to analyze the master tap.

### Browser music views and renderer recovery

Open `http://127.0.0.1:18765/?music=1` after building the browser bundle and
starting the launcher with `-Web`. The view dropdown contains Stage (3D), Piano
roll, Wordfall, Spiral, Pitch wheel, Scope, Spectrum, Vectorscope and Control
curves. Filter event views by lane. Drag flat event views to pan, wheel to zoom,
and double-click to reset. Piano roll can show thin original-pitch traces.

Scope has rising-edge alignment; Spectrum uses logarithmic bands, smoothing and
peak hold; Vectorscope plots stereo mid/side and reports correlation. Audio
comes from the master tap before master gain/limiting, regardless of the lane
filter. Display gain affects visualization only. Control curves show values
sampled at scheduled note onsets, not continuously queried LFOs.

Stage has a global pool limit of 1,024 note blocks, including source traces,
and accepts at most 64 lanes. Retired geometry uses nonsingular transforms.
Nonfinite event values are discarded. GPU drawing pauses while a flat view is
selected. Worker exceptions appear in the HUD; a missing renderer heartbeat
switches to Piano roll after ten seconds (thirty seconds during startup).
Reload the page to restart a failed GPU worker. Flat and audio views run on the
page thread and remain available without WebGPU. Background suspension does not
immediately trip the heartbeat watchdog on return.

`check-nightshade.mjs` exercises the actual browser bundle using an isolated
Chrome DevTools target. It checks all eight additional views, returning to Stage,
a 64-lane/2,048-event stress score, deliberately terminating the test worker, and
music mode without WebGPU. Run it against an isolated headless Chrome instance
with remote debugging on port 19222 and a live demo/hub. Set `CDP_PORT` and
`VIEWER_URL` to override the defaults. It terminates that test tab's worker and
writes ignored screenshots/results in `music-link`; never aim it at a tab you
want to keep using.

Browser session discovery expires publishers after three seconds and selects a live
instance when the selected Rustel process exits. This prevents the stale-stream
state from remaining attached to an old process after a restart.

The browser Spiral now matches Bespoke's time spiral: angle follows event time
relative to the playhead, radius grows across the two-cycle window, and pitch
adds a small radial offset. Curved trails represent duration; pulsing heads mark
onsets; thin outlines show original-pitch traces. Pitch wheel remains a separate
view with twelve fixed pitch-class directions and octave-dependent radius.

### Solo selected lane

Both viewers replace the source-trace toggle with **Solo**. Select a lane first,
then check Solo to audition that lane in the running Rustel session. Changing
lanes while solo is active moves audition to the new lane. Unchecking Solo or
selecting All lanes restores normal playback. Solo is one shared setting per
Rustel session; both viewers reflect the current selected lane's solo state.
Other lanes remain in telemetry so visualization and lane selection keep working.

Solo uses the existing generation-checked command/acknowledgement channel and
never changes lane transpose values or edits the score. Like transpose, it
filters newly scheduled onsets: queued notes, sustained voices and effect tails
may finish before the audible result settles. Score-authored mute/solo rules
still apply. A viewer closing does not automatically clear session-wide solo;
uncheck it explicitly in either viewer. A lane removed from the score clears
its solo automatically. Native HTTP clients can POST `/solo` with `request`,
`session`, `generation`, `lane` and `solo` (boolean); browser clients publish the
same payload on `music/command`. Snapshots expose `solo_lane` (nullable string).

### Typed producer observation

The bridge reads an optional typed `ScheduleTraceEvent` attached to scheduler
output before report JSON conversion. Host lane provenance lives in Hap context,
not ordinary sound controls. Source ranges never pass through temporary
`__rustel_context` JSON fields. Existing plugin routing retains its legacy lane
control key. The editor trace queue is a separate bounded consumer and remains
available when the bridge is enabled. The external music protocol is unchanged.
See [the architecture notes](contribution/ARCHITECTURE.md#typed-telemetry-boundary).
