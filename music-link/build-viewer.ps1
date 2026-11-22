$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$viewer = Join-Path $workspace 'nightshade-viewer'
$bindgen = Join-Path $PSScriptRoot 'tools/bin/wasm-bindgen.exe'
if (!(Test-Path -LiteralPath $bindgen)) {
    throw 'Install workspace wasm-bindgen first: cargo install wasm-bindgen-cli --version 0.2.115 --root music-link/tools --locked'
}
$oldToolchain = $env:RUSTUP_TOOLCHAIN
$oldColor = $env:NO_COLOR
$oldPath = $env:PATH
Push-Location $viewer
try {
    $env:RUSTUP_TOOLCHAIN = 'stable'
    $env:NO_COLOR = 'true'
    $env:PATH = (Split-Path -Parent $bindgen) + ';' + $oldPath
    cargo build -p worker --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { throw 'Worker build failed' }
    & $bindgen --target web --out-dir runtime --out-name engine target/wasm32-unknown-unknown/debug/worker.wasm
    if ($LASTEXITCODE -ne 0) { throw 'Worker bindings failed' }
    npm run build:css
    if ($LASTEXITCODE -ne 0) { throw 'CSS build failed (run npm ci in nightshade-viewer)' }
    trunk build
    if ($LASTEXITCODE -ne 0) { throw 'Viewer bundle failed' }
} finally {
    Pop-Location
    $env:RUSTUP_TOOLCHAIN = $oldToolchain
    $env:NO_COLOR = $oldColor
    $env:PATH = $oldPath
}
