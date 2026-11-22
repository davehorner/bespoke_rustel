$ErrorActionPreference = 'Stop'
$deadline = (Get-Date).AddSeconds(10)
$frame = $null
do {
    $frames = Invoke-RestMethod 'http://127.0.0.1:9614/analysis' -TimeoutSec 2
    $frame = $frames | Where-Object { $_.scope.Count -eq 512 -and $_.spectrum.Count -eq 512 -and $_.sides.Count -gt 0 } | Select-Object -First 1
    if (!$frame) { Start-Sleep -Milliseconds 100 }
} while (!$frame -and (Get-Date) -lt $deadline)
if (!$frame) { throw 'No complete master audio frame. Run the demo with the updated Rustel and hub.' }
$peak = ($frame.scope | ForEach-Object { [math]::Abs($_) } | Measure-Object -Maximum).Maximum
if ($peak -lt 0.00001) { throw 'The demo master tap contains no audible signal.' }
if (($frame.spectrum | Measure-Object -Maximum).Maximum -le -100) { throw 'Spectrum stayed at the silence floor.' }
$next = $null
$deadline = (Get-Date).AddSeconds(5)
do {
    Start-Sleep -Milliseconds 100
    $next = Invoke-RestMethod 'http://127.0.0.1:9614/analysis' -TimeoutSec 2 |
        Where-Object { $_.session -eq $frame.session -and $_.sequence -gt $frame.sequence -and $_.end_frame -gt $frame.end_frame } |
        Select-Object -First 1
} while (!$next -and (Get-Date) -lt $deadline)
if (!$next) { throw 'Audio frame sequence or device clock did not advance.' }
Write-Output "PASS: scope=$($frame.scope.Count), spectrum=$($frame.spectrum.Count), stereo=$($frame.sides.Count), sample_rate=$($frame.sample_rate), sequence=$($frame.sequence)->$($next.sequence), peak=$([math]::Round($peak,4))"
