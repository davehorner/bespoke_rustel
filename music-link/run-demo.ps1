$env:RUSTEL_MUSIC_LINK = '1'
$workspace = Split-Path -Parent $PSScriptRoot
& (Join-Path $workspace 'rustel/target/debug/rustel.exe') play (Join-Path $PSScriptRoot 'demo.strudel') --watch --no-update-check
