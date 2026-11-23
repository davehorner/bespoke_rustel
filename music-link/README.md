# Live Rustel, Bespoke and Nightshade

This first implementation discovers the `$:` lanes of an independently running
Rustel session, lets a Bespoke node transpose a selected lane, and displays
source/output events in Nightshade Viewer. Rustel continues rendering the sound.
Playback stays in Rustel; telemetry includes audio-analysis frames. No second
pattern evaluation is performed.

See [the upstream contribution package](contribution/README.md) for screenshots,
architecture, reproduction steps and discussion/PR drafts.

## Start

Double-click `start_music.cmd` at the workspace root, or run:

```powershell
./music-link/start_music.ps1
```

The launcher builds missing components, starts or reuses the hub, watched demo
player, native Bespoke viewer and Nightshade web server, and waits for live lanes
before reporting ready. It prints `http://127.0.0.1:18765/?music=1` and opens
Nightshade in your browser on every launch, including when the server is reused.
Run it again without duplicating those apps. It does not close existing
patches or replace an existing Rustel session's score.
For demo purposes, the launcher disables the welcome overlay in its dedicated
Bespoke preferences file. Normal Bespoke preferences are unchanged.
The demo patch opens with `rustel source` already connected to `transpose`;
select a live lane in the source node to use it.

After pulling integration updates, the launcher applies the current vendored
sources and rebuilds binaries and the web viewer when their inputs are newer.
It upgrades only recognized earlier integration versions; conflicting local edits
stop setup before it changes any project. Save and close running apps before a
rebuild. The demo patch is loaded on a fresh Bespoke launch.

## Named pattern colors

With the music bridge enabled, top-level `const` pattern bindings automatically
name their events. No instrument-name configuration is needed:

```javascript
const moon = note("c3 e3").s("sine")
const cedar = note("g4").s("triangle")
const piece = arrange([4, stack(moon, cedar)], [4, moon])
$: piece
```

Bespoke and Nightshade color event boxes by `moon` or `cedar` and label them
with those names. A deterministic hash of the UTF-8 name generates the color;
RGB values travel with the events so both viewers agree. Moving declarations
or filtering lanes keeps colors stable. Renaming a variable changes its color.
Source const names use the same colors.

A derived part replaces its helper's name (`const guitar = harmony.voicing()`
becomes `guitar`). Bindings composed with `stack` or `arrange` preserve named
children and give their own name only to unnamed events. Unnamed events retain
lane colors. This is visual provenance within an existing lane, so audio outputs,
solo and transpose still operate on playback lanes. Top-level identifier const
bindings are supported; destructuring and declarations inside functions are not
automatically named. Generated hues can occasionally be similar.

## Expand arrangements into virtual lanes

Each `$:` playback lane can expand independently into its named parts. For example:

```javascript
const bass = note("c2 g2").s("sine")
const lead = note("e4 g4").s("triangle")
const verse = arrange([4, stack(bass, lead)], [4, bass])
const drums = stack(s("bd*4"), s("hh*8"))
$: verse
$: drums
```

The first parent line is labeled `verse`; expanding it creates `bass` and `lead`
tracks. The second parent can expand separately. Reusing a part in two parents
creates a track under each, without merging their events. Inline unnamed events
stay on the parent line. Collapsing puts all events back on their parent.

In Nightshade, click **+** / **−** beside a lane-card label or on its 3D core-line
heading. Lanes with zero discovered parts have no expansion control. Clicking
the card's source disclosure still opens source code independently. In Bespoke,
click `[+]` / `[-]` on a Stage core-line label, or use **parts** for the selected
parent (**all lanes** applies the toggle to every parent). Stage, piano roll and
wordfall separate the tracks spatially; other event views retain their shared
geometry and part colors. Drag to pan and zoom to inspect larger arrangements.
Event-name labels have transparent backgrounds.
Nightshade also prints note names (`C4`, `F#3`, etc.) on all six 3D block faces
and inside piano-roll blocks; unpitched hits show `x`. Fractional pitches retain
their cent offsets. Glyphs fit the block footprint without a background plate;
zoom in to read very short notes. Part names remain above the 3D blocks and on
their virtual-lane headings. Wordfall uses the same note/hit notation.
Bespoke's Stage also labels every block face with this note/hit notation.
Glyphs follow each projected face and its depth order as you rotate the stage,
without mirrored text or hidden-side labels bleeding through. Short notes have
smaller face labels; zoom in for readability. Part-name labels remain separate.
In Bespoke, clicking a numbered lane label or an expanded part-name label scrolls
the existing evaluated-document pane to its definition and highlights the target.
Stage headings and floating part labels support the same navigation. The small
`[+]` / `[-]` region remains dedicated to expanding or collapsing parts.
In Nightshade, core rails also carry flat text inscriptions. Virtual-part rails are shorter
and inset beneath their playback parent. Time-axis captions and ticks sit in
their own gutter, separate from the rails; crowded axis values are suppressed
instead of overlapping captions or one another.

Click a lane name, rail, ball or pulse ring to reveal its source in the existing
lane card. A named part opens the evaluated document, highlights its `const`
name and scrolls to that definition. Selecting a playback lane in the dropdown
also reveals its expression or referenced const. The small **+** / **−** on a
parent heading still expands/collapses parts. This navigation does not change
solo or audio routing, and does not require a new editor panel or file access.

Parts are discovered from the events Rustel actually schedules, without querying
patterns a second time. They stay registered through rests and repeated cycles
until the score generation changes; a part in a later section appears when that
section is first scheduled. Names are ordered by their declarations in the score.
These are visual lanes: solo, transpose and `rustelaudio` still address the `$:`
playback parent. Virtual IDs are never sent as audio/control targets.

Options: `-Status` checks connections; Nightshade is enabled by default (`-Web`
is still accepted). `-NoWeb` disables it, and `-NoOpen` serves it and prints its
URL without opening a browser;
`-Score path.strudel` chooses the score for a new player; `-Silent` starts a new
player without audio hardware; `-Build` explicitly rebuilds components (close the
affected apps first). Build tools and platform dependencies must be installed.
Logs are under `music-link/logs`.

`-Stop` stops only processes this launcher started, verifying their PID, start
time and executable path. Reused apps are left running. Bespoke receives a normal
close request so it can handle unsaved work. Multiple simultaneous launcher runs
are prevented by a file lock.

### macOS and Linux

From the workspace root:

```sh
./Setup.sh
./start_music.sh
./start_music.sh --no-open
./start_music.sh --score "/path with spaces/song.strudel"
./start_music.sh --status
./start_music.sh --stop
```

macOS users can also double-click `start_music.command`. Both wrappers use
Python 3.9+ and need no PowerShell. `sh Setup.sh` and
`sh start_music.sh` work if a download did not preserve executable permissions.

Install Git, Python 3 with development headers, the Rust toolchain required by
Rustel, CMake and a native C/C++ toolchain first. On macOS, install the Xcode
command-line tools. Linux also needs `pkg-config` and development libraries for
audio and the desktop UI. Follow the platform lists in
[Rustel's build guide](../rustel/docs/building.md) and
[Bespoke's build guide](../BespokeSynth/README.md#building); its
[Linux dependency list](../BespokeSynth/azure-pipelines.yml) supplies the native
packages. The launcher does not run sudo or install system packages.

Nightshade (enabled by default) additionally needs Node.js/npm, rustup and Trunk (`cargo install trunk
--locked`). The launcher installs its pinned local wasm-bindgen CLI, adds the
WASM Rust target, runs `npm ci` if needed, and builds the worker/CSS/web bundle.
It serves only on loopback. Use `--no-open` to print the URL without opening a
browser, `--no-web` to skip Nightshade entirely, or `--web-port 18766` to choose
another port. Every normal launch opens the printed music-view URL, even when
reusing the server. An unrelated server on the
chosen port is left alone and reported as a conflict.

Native builds use `BespokeSynth/ignore/music-build-posix`, separate from the
Windows Visual Studio build. The launcher finds either the Linux executable or
the macOS `.app` executable. `--build` explicitly rebuilds components; ordinary
startup builds only missing outputs. Close the affected apps before rebuilding.
`--no-bespoke --web` runs just Rustel, the hub and Nightshade. `--silent` renders
Rustel on its silent device for hardware-free observation; it produces no sound.

The launcher reuses the same workspace's existing processes and preserves their
settings and score. Runtime state is in `music-link/launcher-posix.json`; logs
are under `music-link/logs`. A file lock prevents concurrent launchers. `--stop`
checks both process start identity and executable, stops only launcher-owned
background processes, and **leaves Bespoke open** so you can save and quit it
normally. Reused external processes are never stopped.

Lane discovery, transpose/Solo, native visualization, Nightshade and per-lane
`rustelaudio` stereo PCM use the same node interface on Windows, macOS and Linux.
Unix startup enables lane audio by default. Use `--no-lane-audio` to keep normal
shared orbit routing, or `--lane-audio` to explicitly require the export. A reused
Rustel must already have the requested export enabled; otherwise restart it.
The native IPC contract check in CI runs on both Linux and macOS; full native
macOS application playback has not been exercised on this Windows development host.

Run the portable regression tests with:

```sh
python3 -m unittest discover -s music-link/tests -p 'test_*.py' -v
```

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


### Per-lane stereo audio in Bespoke (Windows, macOS, Linux)

`rustelaudio` is a normal Bespoke audio source. Add one node for each lane you
want to process, choose its Rustel instance/lane, and connect its audio cable to
Bespoke filters, delays, mixers or an output. Multiple nodes may receive the same
lane; their output will sum if you connect them to the same destination.

Build the updated hub, Rustel and Bespoke using the existing build instructions.
Start a fresh Rustel process with lane export enabled (Windows):

```powershell
.\music-link\start_music.ps1 -LaneAudio
```

Use `-LaneAudio -Silent` to render through Rustel's silent output while all
listening happens in Bespoke. A previously running Rustel must be restarted;
changing the launcher's environment cannot change an existing process.
For Studio or a manual CLI launch, set both `RUSTEL_MUSIC_LINK=1` and
`RUSTEL_LANE_AUDIO=1` in its launching environment.

The first 15 distinct lane identities get dedicated stereo orbit slots. Slots
remain reserved for the session, even after removal from the score, so existing
voice, delay, reverb and insert-effect tails continue in their original output.
Retired lanes remain selectable as `(retired lane / tails)`. Further lane
identities still play through Rustel's unexported orbit, but are not available as
separate outputs; restart the session to clear its slot reservations.

This is an explicit isolated-orbit mode. It replaces score-authored `.orbit()`
assignments for exported lanes and places unexported score voices on orbit 15.
Shared cross-lane orbit effects, orbit-number-based ducking and orbit hardware
routing therefore do not retain their ordinary shared-bus meaning in this mode.
Exported stems are post lane/orbit effects, channel mapping, duck and orbit
fader, **before Rustel's master gain and limiter**. Their independent effects
keep processing between notes; this does not reconstruct audio from event data.
Normal Rustel stop/score-takeover rules still apply. Keyboard audition is not a
score lane output.

Hearsay/HTTP carries discovery only. PCM uses versioned native shared memory (Windows named mappings or POSIX
`shm_open`/`mmap` objects on macOS/Linux),
15 stereo rings of 16,384 frames, and tagged atomic samples. A Bespoke worker
reads and linearly resamples each selected stream with a bounded clock-drift
adjustment. There is about 50 ms of source buffering plus three Bespoke blocks;
this first implementation is not sample-aligned to unrelated Bespoke sources or
an external clock. Different nodes may have slightly different startup alignment.
No network, mapping operations, waits or locks run in the Bespoke audio callback.
Its fixed queue fades underflow/recovery instead of replaying old blocks.

A connected, enabled node renews its lane's monitor lease while PCM is advancing.
Rustel removes that lane from its direct monitor to prevent doubled playback.
On disconnect, disable, process exit or stalled reading, the lease expires within
500 ms and direct monitoring returns (a silent Rustel device remains silent).
A brief gap can occur when first connecting while the receiver buffers. Shared
memory is reclaimed when both processes release it; no PCM recordings are saved.
The node remembers its selected session/lane in Bespoke's saved state; a new
Rustel process has a new session ID and must be selected again.

On macOS/Linux, use `./start_music.sh` (export is enabled by default), or
set `RUSTEL_MUSIC_LINK=1 RUSTEL_LANE_AUDIO=1` for a manual launch. POSIX objects
are created exclusively with mode 0600, use short names compatible with macOS,
and are unlinked on normal producer destruction. Existing readers may finish
using an unlinked mapping. The launcher cleans up after stopping its owned
producer, and readers unlink streams when they detect a dead producer. A hard
crash with no reader or launcher present can leave a POSIX object until OS
cleanup; no audio files are written. Leases use the same OS monotonic clock on
both peers, and all shared sample access uses lock-free 64-bit atomics.

Hearsay remains the discovery/control transport. Its best-effort broker queues
are useful for event telemetry but do not replace the PCM buffer's sample clock,
resampling, gap handling and monitor leases.


Verification commands (after building) are:

```powershell
cargo test --manifest-path rustel/Cargo.toml -p rustel-audio --features device-audio lane_audio::tests --lib -- --test-threads=1
cargo test --manifest-path rustel/Cargo.toml -p rustel-runtime --no-default-features --features music-link session::music::tests --lib
python music-link/check-lane-audio.py
python music-link/check-lane-audio.py --bespoke
```

The last check launches a temporary Bespoke patch with two `rustelaudio` nodes
connected to inaudible test sinks, verifies callback consumption through the
shared-memory diagnostics, and closes only processes it created. Run it with no
other Bespoke instance open. The producer check uses Rustel's silent output and
also verifies that removing a lane does not recycle its export slot. Existing
build and protocol checks cover normal playback with export disabled.


For a headless Rust-writer/C++-reader IPC contract test on Windows, macOS or Linux:

```sh
python3 music-link/tests/check_native_ipc.py
```

This compiles the shipping vendored writer/reader with deterministic sample
blocks and checks tagged stereo reads, lease mute/recovery and mapping cleanup
after normal exit or a hard crash. It needs stable Rust, a C++17 compiler and
Python 3, but no audio SDKs or device. Windows uses CMake with the Visual Studio C++ tools
and the Windows SDK; it detects the installed Visual Studio version. Run it with
`python` rather than `python3` if needed. The
portable CI matrix runs the IPC check on all three operating systems. The full application probe is
`python3 music-link/check-lane-audio.py --bespoke` after native builds.
