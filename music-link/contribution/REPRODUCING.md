# Reproduce and verify the prototype

## Checkout layout

Use these sibling directories under one workspace:

```text
workspace/
  BespokeSynth/
  rustel/
  nightshade-viewer/
  hearsay/
  music-link/
```

The screenshots use the uncommitted integration working trees described in
[manifest.json](manifest.json), not stock upstream checkouts. Cloning only the
base commits will not include these changes. The demo is [demo.strudel](demo.strudel).
Run the commands below from the workspace root.

## Windows prerequisites

The tested environment is Windows x64, Cargo/Rust 1.100.0, CMake 3.29.2, Visual
Studio 2022 C++ build tools and Node 25.2.1. Use the projects' own prerequisite
instructions for system libraries. Initialize Bespoke submodules and retain
Python support for the currently tested build. Other OSes and alternate Bespoke
feature combinations have not been validated for this integration.

Nightshade needs the wasm32 target, npm dependencies, Trunk and wasm-bindgen CLI
0.2.115 matching its pinned dependency. The shared crate currently depends on
local Hearsay paths. Read the checked-out CONTRIBUTING files before changing or
submitting code.

## Build

```powershell
git -C BespokeSynth submodule update --init --recursive
cargo build --manifest-path music-link/Cargo.toml --bin music-hub
cargo build --manifest-path rustel/Cargo.toml -p rustel --no-default-features --features studio,music-link
cmake -S BespokeSynth -B BespokeSynth/ignore/music-build -A x64
cmake --build BespokeSynth/ignore/music-build --config Release --target BespokeSynth --parallel 4
rustup target add wasm32-unknown-unknown
npm --prefix nightshade-viewer ci
cargo install wasm-bindgen-cli --version 0.2.115 --root music-link/tools --locked
./music-link/build-viewer.ps1
```

Install Trunk using the project's documented tooling if it is not available.
Close the affected existing processes before rebuilding their standard binaries;
do not work around Windows executable locks by renaming outputs.

## Start and exercise

```powershell
./music-link/start_music.ps1 -Web
```

The launcher starts or reuses the hub, watched nine-lane demo and native Bespoke
layout. The browser URL is `http://127.0.0.1:18765/?music=1`. Reused Rustel
instances keep their current score. Start a new instance or use its existing
watched score intentionally rather than assuming the launcher replaces it.

The runtime must be compiled with `music-link`, with `RUSTEL_MUSIC_LINK=1` set
before score evaluation. The launcher sets this for its new player.

1. Confirm nine lanes appear and change as the demo runs.
2. Select a lane in Bespoke `rustelpattern`, connect it to `rusteltranspose`,
   then set an offset. Only newly scheduled events receive that offset.
3. Select the same lane in either viewer and enable Solo. Rustel auditions that
   lane while retaining all visual telemetry. Change lanes and confirm audition
   follows; turn Solo off to restore normal playback.
4. Compare the time-based Spiral with the pitch-class wheel. Check waveform,
   spectrum and stereo views. Their display gain does not alter sound.
5. Edit the watched score and confirm generation and source updates. Native
   source highlights use the full document's byte offsets.

The score uses built-in sine/triangle synths and needs no sample downloads.
Long notes and effect tails can finish after Solo changes.

## Focused checks already used

```powershell
cargo test --manifest-path music-link/Cargo.toml --lib
cargo test --manifest-path rustel/Cargo.toml -p rustel-scheduler --lib
cargo test --manifest-path rustel/Cargo.toml -p rustel-core --lib
cargo test --manifest-path rustel/Cargo.toml -p rustel-jsruntime --lib
cargo test --manifest-path rustel/Cargo.toml -p rustel-runtime --no-default-features --features music-link --test music_telemetry
cargo test --manifest-path rustel/Cargo.toml -p rustel-runtime --no-default-features --features music-link session::music::tests --lib
$env:RUSTUP_TOOLCHAIN='stable'
cargo check --manifest-path nightshade-viewer/Cargo.toml -p worker -p nightshade-viewer --target wasm32-unknown-unknown --features agent
```

`music-link/check-solo.mjs` checks HTTP and real browser checkbox commands against
a live demo, restoring normal playback afterward. `check-nightshade.mjs` tests
views, a 64-lane/2,048-event stress score, worker termination/fallback and no-WebGPU
operation. These use an isolated Chrome DevTools target on port 19222; do not
point the freeze test at a tab you intend to keep using.

`capture-browser.mjs` writes the ten gallery images through the same isolated
browser endpoint. It requires a running hub/player and static server, temporarily
solos a lane, and restores the prior solo setting. Screenshot metadata is in the
manifest. Runtime/browser profiles and temporary capture data are excluded from
this package.

## Before an upstream PR

Focused checks are not the complete upstream test suite. In Rustel run the exact
checks in its CONTRIBUTING.md, including formatting, engine/unwind/release/security
scripts, all-feature Clippy and workspace tests, release runtime e2e, the product
build, Studio PTY e2e and Windows installer-security checks. Resolve known build
warnings before a `-D warnings` run. Run platform-specific suites where required.

For Bespoke apply its Clang-format rules, build the intended configurations and
attach a savestate/layout and native screenshots. Add user-facing module/tooltips
documentation as requested by that repository. For Nightshade verify ordinary
model-viewer startup, both agent settings, music mode and worker/page schema
compatibility. Publish or pin the shared protocol dependency before claiming any
individual PR builds from a fresh standalone checkout.

An isolated browser endpoint can be started with Chrome from a separate terminal:

```powershell
chrome --headless=new --remote-debugging-port=19222 --user-data-dir=music-link/browser-test-profile about:blank
node music-link/contribution/capture-browser.mjs
```

Use a dedicated profile and leave the normal browser separate. The screenshot
script waits for renderer startup and brings its own test tab forward before
capturing Stage. It closes only that capture tab afterward.

`music-link/check-telemetry.mjs` verifies live source byte ranges, controls and
transpose through protocol v1, restoring the prior transpose offset afterward.
