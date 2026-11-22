$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$hubPath = Join-Path $PSScriptRoot 'target/debug/music-hub.exe'
$probePath = Join-Path $workspace 'rustel/target/debug/examples/music_link.exe'
$hub = $null
$probe = $null
$previous = $env:RUSTEL_MUSIC_LINK
try {
    $hub = Start-Process -FilePath $hubPath -PassThru -WindowStyle Hidden
    $env:RUSTEL_MUSIC_LINK = '1'
    $probe = Start-Process -FilePath $probePath -PassThru -WindowStyle Hidden
    $deadline = (Get-Date).AddSeconds(15)
    $snapshot = $null
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 150
        try {
            $sessions = Invoke-RestMethod 'http://127.0.0.1:9614/sessions' -TimeoutSec 2
            $snapshot = $sessions | Where-Object { $_.lanes.Count -eq 2 -and $_.events.Count -gt 0 } | Select-Object -First 1
            if ($snapshot) { break }
        } catch { }
    }
    if (!$snapshot) { throw 'No two-lane Rustel snapshot received' }
    $pitched = $snapshot.events | Where-Object { $null -ne $_.original_pitch } | Select-Object -First 1
    if (!$pitched) { throw 'No pitched events received' }
    $command = @{request=[guid]::NewGuid().ToString();session=$snapshot.session;generation=$snapshot.generation;lane=$pitched.lane;semitones=7}
    $response = Invoke-RestMethod 'http://127.0.0.1:9614/transpose' -Method Post -ContentType 'application/json' -Body ($command | ConvertTo-Json) -TimeoutSec 3
    if (!$response.accepted) { throw "Transpose rejected: $($response.message)" }
    $deadline = (Get-Date).AddSeconds(5)
    $changed = $null
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 100
        $sessions = Invoke-RestMethod 'http://127.0.0.1:9614/sessions' -TimeoutSec 2
        $changed = $sessions.events | Where-Object { $null -ne $_.original_pitch -and [math]::Abs($_.pitch - $_.original_pitch - 7) -lt 0.001 } | Select-Object -First 1
        if ($changed) { break }
    }
    if (!$changed) { throw 'Acknowledged transpose did not reach scheduled events' }
    $command.generation = $snapshot.generation + 1
    try {
        Invoke-RestMethod 'http://127.0.0.1:9614/transpose' -Method Post -ContentType 'application/json' -Body ($command | ConvertTo-Json) -TimeoutSec 3
        throw 'Stale command was accepted'
    } catch {
        if ($_.Exception.Response.StatusCode.value__ -ne 409) { throw }
    }
    Write-Output "PASS: two live lanes, event stream, acknowledged transpose +7, stale generation rejected"
} finally {
    $env:RUSTEL_MUSIC_LINK = $previous
    if ($probe -and !$probe.HasExited) { Stop-Process -Id $probe.Id -Force }
    if ($hub -and !$hub.HasExited) { Stop-Process -Id $hub.Id -Force }
}
