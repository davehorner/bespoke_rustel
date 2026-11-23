#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$Score = (Join-Path $PSScriptRoot 'strudel/demo_arrangement.strudel'),
    [switch]$Web = $true,
    [switch]$NoWeb,
    [switch]$NoOpen,
    [int]$WebPort = 18765,
    [switch]$Silent,
    [switch]$LaneAudio,
    [switch]$Build,
    [switch]$Status,
    [switch]$Stop
)

$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$statePath = Join-Path $PSScriptRoot 'launcher-state.json'
$logDirectory = Join-Path $PSScriptRoot 'logs'
$lock = $null
$script:LaunchState = @{ version = 1; workspace = $workspace; processes = @(); webOpened = $false }

function Save-LauncherState {
    $temporary = $statePath + '.tmp'
    $script:LaunchState | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $temporary
    [System.IO.File]::Move($temporary, $statePath, $true)
}

function Get-RecordedProcess($Record) {
    $process = Get-Process -Id $Record.pid -ErrorAction SilentlyContinue
    if (!$process) { return $null }
    try {
        if ($process.Path -ne $Record.path -or $process.StartTime.ToUniversalTime().Ticks.ToString() -ne $Record.started) { return $null }
    } catch { return $null }
    return $process
}

function Get-HubState {
    try {
        $reply = Invoke-WebRequest 'http://127.0.0.1:9614/sessions' -TimeoutSec 2 -NoProxy
        $sessions = ConvertFrom-Json -InputObject $reply.Content -NoEnumerate
        if ($sessions -isnot [System.Array]) { throw 'The listener on port 9614 is not the music hub.' }
        foreach ($session in $sessions) {
            if ($session.version -ne 1 -or !$session.session) { throw 'Unexpected session protocol on port 9614.' }
        }
        return @{ healthy = $true; sessions = @($sessions) }
    } catch { return @{ healthy = $false; problem = $_.Exception.Message; sessions = @() } }
}

function Wait-Until([scriptblock]$Check, [int]$Seconds, [string]$Failure) {
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    do {
        if (& $Check) { return }
        Start-Sleep -Milliseconds 200
    } while ($clock.Elapsed.TotalSeconds -lt $Seconds)
    throw $Failure
}

function Invoke-Build([string]$Tool, [string[]]$Arguments) {
    if (!(Get-Command $Tool -ErrorAction SilentlyContinue)) { throw "Required build tool '$Tool' is missing." }
    Write-Host "Building: $Tool $($Arguments -join ' ')" -ForegroundColor Cyan
    & $Tool @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Build failed ($Tool, exit $LASTEXITCODE). Close the affected app before rebuilding. No running apps were terminated." }
}

function Test-NeedsBuild([string]$Project, [string]$Executable) {
    if ($Build -or !(Test-Path -LiteralPath $Executable)) { return $true }
    $result = & python (Join-Path $PSScriptRoot 'build_inputs.py') $Project $Executable
    if ($LASTEXITCODE -ne 0) { throw 'Could not check build freshness.' }
    return $result -eq 'true'
}

function Prepare-Rebuild([string]$Executable) {
    $name = [System.IO.Path]::GetFileNameWithoutExtension($Executable)
    foreach ($process in @(Get-Process -Name $name -ErrorAction SilentlyContinue)) {
        if ($process.Path -ne $Executable) { continue }
        if ($name -eq 'BespokeSynth') {
            Write-Host 'Bespoke source was updated. Requesting normal close before rebuilding; save any patch if prompted.'
            $null = $process.CloseMainWindow()
            if ($process.WaitForExit(3000)) { continue }
        }
        throw "$name needs rebuilding. Save your work and close it, then rerun start_music.cmd. The old executable will not be reused."
    }
}

function Ensure-Process([string]$Role, [string]$Executable, [string[]]$Arguments, [string[]]$Names, [string[]]$AcceptPaths) {
    foreach ($candidate in @(Get-Process -Name $Names -ErrorAction SilentlyContinue | Sort-Object @{Expression={try { if ($_.Path -eq $Executable) { 0 } else { 1 } } catch { 1 }}},Id)) {
        try { $path = $candidate.Path } catch { continue }
        if ($path -notin $AcceptPaths) { continue }
        $previous = @($script:LaunchState.processes | Where-Object { $_.role -eq $Role -and $_.pid -eq $candidate.Id })
        $owned = $previous.Count -gt 0 -and $previous[0].owned -and $null -ne (Get-RecordedProcess $previous[0])
        $record = @{ role = $Role; pid = $candidate.Id; path = $path; started = $candidate.StartTime.ToUniversalTime().Ticks.ToString(); owned = [bool]$owned }
        $script:LaunchState.processes = @($script:LaunchState.processes | Where-Object { $_.role -ne $Role }) + @($record)
        Save-LauncherState
        Write-Host "Reusing $Role (PID $($candidate.Id))" -ForegroundColor DarkCyan
        if ($Role -eq 'bespoke' -and $path -ne $Executable) {
            Write-Warning 'Bespoke is using an older executable. Close it and rerun the launcher to use the latest native viewer.'
        }
        return $candidate
    }
    $outputLog = Join-Path $logDirectory "$Role.log"
    $errorLog = Join-Path $logDirectory "$Role-error.log"
    # Start-Process joins ArgumentList; quote each argument for Windows paths.
    $quoted = @($Arguments | ForEach-Object { '"' + $_.Replace('"', '\"') + '"' })
    $options = @{ FilePath = $Executable; WorkingDirectory = $workspace; PassThru = $true }
    if ($quoted.Count) { $options.ArgumentList = $quoted }
    if ($Role -ne 'bespoke') {
        $options.WindowStyle = 'Hidden'
        $options.RedirectStandardOutput = $outputLog
        $options.RedirectStandardError = $errorLog
    }
    $process = Start-Process @options
    $record = @{ role = $Role; pid = $process.Id; path = $Executable; started = $process.StartTime.ToUniversalTime().Ticks.ToString(); owned = $true }
    $script:LaunchState.processes = @($script:LaunchState.processes | Where-Object { $_.role -ne $Role }) + @($record)
    Save-LauncherState
    Start-Sleep -Milliseconds 300
    $process.Refresh()
    if ($process.HasExited) {
        $tail = if (Test-Path -LiteralPath $errorLog) { Get-Content -LiteralPath $errorLog -Tail 8 | Out-String } else { '' }
        throw "$Role exited ($($process.ExitCode)). $tail"
    }
    Write-Host "Started $Role (PID $($process.Id))" -ForegroundColor Green
    return $process
}

function Start-NightshadeViewer {
    $dist = Join-Path $workspace 'nightshade-viewer/dist'
    if ($WebPort -lt 1024 -or $WebPort -gt 65535) { throw 'WebPort must be between 1024 and 65535.' }
    if (Test-NeedsBuild 'nightshade-viewer' (Join-Path $dist 'index.html')) {
        $bindgen = Join-Path (Join-Path $workspace 'music-link') 'tools/bin/wasm-bindgen.exe'
        if (!(Test-Path -LiteralPath $bindgen)) {
            Invoke-Build 'cargo' @('install', 'wasm-bindgen-cli', '--version', '0.2.115', '--root', (Join-Path (Join-Path $workspace 'music-link') 'tools'), '--locked')
        }
        if (!(Test-Path -LiteralPath (Join-Path $workspace 'nightshade-viewer/node_modules'))) {
            Push-Location (Join-Path $workspace 'nightshade-viewer')
            try { Invoke-Build 'npm' @('ci') } finally { Pop-Location }
        }
        & (Join-Path (Join-Path $workspace 'music-link') 'build-viewer.ps1')
    }
    $url = "http://127.0.0.1:$WebPort/"
    $served = $false
    try { $served = (Invoke-WebRequest $url -TimeoutSec 2 -NoProxy).Content.Contains('<title>Nightshade Viewer</title>') } catch { }
    if (!$served) {
        $python = (Get-Command python -ErrorAction Stop).Source
        $null = Ensure-Process 'web' $python @('-m', 'http.server', "$WebPort", '--bind', '127.0.0.1', '--directory', $dist) @('__no_existing_python_match__') @()
        Wait-Until { try { (Invoke-WebRequest $url -TimeoutSec 2 -NoProxy).StatusCode -eq 200 } catch { $false } } 10 "Viewer server failed. Port $WebPort may be occupied; use -WebPort to choose another."
    }
    Write-Host "Nightshade viewer: $($url)?music=1" -ForegroundColor Green
    if (!$NoOpen) {
        try { Start-Process ($url + '?music=1') }
        catch { Write-Warning "Browser could not open. Use the Nightshade URL printed above. $($_.Exception.Message)" }
    }
}


try {
    try { $lock = [System.IO.File]::Open((Join-Path $PSScriptRoot 'launcher.lock'), 'OpenOrCreate', 'ReadWrite', 'None') }
    catch { throw 'Another launcher is already working. Wait for it to finish, then try again.' }
    if (Test-Path -LiteralPath $statePath) {
        $saved = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
        if ($saved.version -ne 1 -or $saved.workspace -ne $workspace) { throw 'Launcher state belongs to a different workspace.' }
        $script:LaunchState = $saved
    }
    if ($Stop) {
        foreach ($record in $script:LaunchState.processes) {
            $process = Get-RecordedProcess $record
            if (!$process) { continue }
            if (!$record.owned) { Write-Host "Leaving reused $($record.role) running (PID $($record.pid))."; continue }
            if ($record.role -eq 'bespoke') {
                # Let Bespoke handle unsaved changes through its normal close flow.
                if (!$process.CloseMainWindow()) { Write-Warning 'Bespoke could not be asked to close. Close it manually.' }
            } else { Stop-Process -Id $process.Id }
            Write-Host "Requested stop: $($record.role) (PID $($record.pid))"
        }
        return
    }
    if ($Status) {
        foreach ($record in $script:LaunchState.processes) {
            $live = $null -ne (Get-RecordedProcess $record)
            Write-Host "$($record.role): $(if ($live) { 'running' } else { 'stopped' }) / PID $($record.pid) / $(if ($record.owned) { 'launcher-owned' } else { 'reused' })"
        }
        $hub = Get-HubState
        if (!$hub.healthy) { Write-Warning "Hub unavailable: $($hub.problem)"; exit 1 }
        foreach ($session in $hub.sessions) { Write-Host "$($session.session): $(@($session.lanes).Count) live lanes" }
        if (!$hub.sessions.Count) { Write-Warning 'Hub is up; no live Rustel lanes. Evaluate a score or start the demo.' }
        return
    }

    if (!(Test-Path -LiteralPath $Score -PathType Leaf)) { throw "Score not found: $Score" }
    $Score = (Resolve-Path -LiteralPath $Score).Path
    New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
    Push-Location $workspace
    try {
        & python (Join-Path $workspace 'vendor/setup.py') --upgrade --ensure
        if ($LASTEXITCODE -ne 0) { throw 'Could not prepare current vendored source; local edits were preserved.' }
        $hubExe = Join-Path $PSScriptRoot 'target/debug/music-hub.exe'
        $rustelExe = Join-Path $workspace 'rustel/target/debug/rustel.exe'
        $bespokeDirectory = Join-Path $workspace 'BespokeSynth/ignore/music-build/Source/BespokeSynth_artefacts/Release'
        $bespokeExe = Join-Path $bespokeDirectory 'BespokeSynth.exe'
        if (Test-NeedsBuild 'music-link' $hubExe) { Prepare-Rebuild $hubExe; Invoke-Build 'cargo' @('build', '--manifest-path', 'music-link/Cargo.toml', '--bin', 'music-hub') }
        if (Test-NeedsBuild 'rustel' $rustelExe) { Prepare-Rebuild $rustelExe; Invoke-Build 'cargo' @('build', '--manifest-path', 'rustel/Cargo.toml', '-p', 'rustel', '--no-default-features', '--features', 'studio,music-link') }
        if (Test-NeedsBuild 'BespokeSynth' $bespokeExe) {
            Prepare-Rebuild $bespokeExe
            if (!(Test-Path -LiteralPath (Join-Path $workspace 'BespokeSynth/libs/JUCE/CMakeLists.txt'))) {
                Invoke-Build 'git' @('-C', 'BespokeSynth', 'submodule', 'update', '--init', '--recursive')
            }
            Invoke-Build 'cmake' @('-S', 'BespokeSynth', '-B', 'BespokeSynth/ignore/music-build', '-G', 'Visual Studio 17 2022', '-A', 'x64', '-DBESPOKE_ENABLE_PYTHON=ON')
            Invoke-Build 'cmake' @('--build', 'BespokeSynth/ignore/music-build', '--config', 'Release', '--parallel', '4')
        }

        $hub = Get-HubState
        if (!$hub.healthy) {
            $null = Ensure-Process 'hub' $hubExe @() @('music-hub') @($hubExe)
            Wait-Until { (Get-HubState).healthy } 15 "Hub did not become ready. See $logDirectory/hub-error.log; check ports 9612–9614."
        } else {
            Write-Host 'Reusing existing music hub at http://127.0.0.1:9614' -ForegroundColor DarkCyan
        }
        $previousLink = $env:RUSTEL_MUSIC_LINK
        $previousLaneAudio = $env:RUSTEL_LANE_AUDIO
        $previousMusicRoot = $env:MUSIC_LINK_ROOT
        $previousSessionFile = $env:RUSTEL_SESSION_FILE
        $previousDemoName = $env:RUSTEL_DEMO_NAME
        $templateScore = $Score
        $existingRustel = @(Get-Process -Name rustel -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $rustelExe })
        if (!$existingRustel.Count) {
            $Score = & python (Join-Path $PSScriptRoot 'session_score.py') $templateScore
            if ($LASTEXITCODE -ne 0) { throw 'Could not create the session score.' }
            Write-Host "Session score: $Score"
        }
        $env:MUSIC_LINK_ROOT = $PSScriptRoot
        $env:RUSTEL_SESSION_FILE = $Score
        $env:RUSTEL_DEMO_NAME = [System.IO.Path]::GetFileName($templateScore)
        $env:RUSTEL_MUSIC_LINK = '1'
        if ($LaneAudio) { $env:RUSTEL_LANE_AUDIO = '1' }
        try {
            $arguments = @('play', $Score, '--watch', '--no-update-check', '--no-input')
            if ($Silent) { $arguments += @('--output', 'silent') }
            $player = Ensure-Process 'rustel' $rustelExe $arguments @('rustel') @($rustelExe)
        } finally {
            $env:RUSTEL_MUSIC_LINK = $previousLink; $env:RUSTEL_LANE_AUDIO = $previousLaneAudio
            $env:MUSIC_LINK_ROOT = $previousMusicRoot; $env:RUSTEL_SESSION_FILE = $previousSessionFile; $env:RUSTEL_DEMO_NAME = $previousDemoName
        }
        $sessionPattern = "rustel-$($player.Id)-*"
        $lanesReady = {
            if ($player.HasExited) { throw "Rustel exited before publishing lanes. See $logDirectory/rustel-error.log." }
            @((Get-HubState).sessions | Where-Object { $_.session -like $sessionPattern -and $_.lanes.Count -gt 0 }).Count -gt 0
        }
        if (!(& $lanesReady)) {
            # Rustel can spend 60 seconds preloading samples before its first publication.
            Write-Host 'Waiting for Rustel lanes. Cold sample downloads can delay startup.'
            Wait-Until $lanesReady 90 "Rustel did not publish lanes within 90 seconds. If reusing Studio, evaluate its score. See $logDirectory/rustel-error.log."
        }
        if ($LaneAudio) {
            Wait-Until { @((Get-HubState).sessions | Where-Object { $_.session -like $sessionPattern -and $null -ne $_.lane_audio }).Count -gt 0 } 5 "No lane audio export. An existing Rustel process may have been started without RUSTEL_LANE_AUDIO=1. Stop that process and launch again with -LaneAudio. Rebuild the updated native audio transport if this persists."
        }

        $preferences = Join-Path $PSScriptRoot 'native-view-prefs.json'
        $demoPreferences = if (Test-Path -LiteralPath $preferences) {
            Get-Content -LiteralPath $preferences -Raw | ConvertFrom-Json -AsHashtable
        } else {
            @{ layout = (Join-Path $PSScriptRoot 'native-view-layout.json').Replace('\', '/'); audio_input_device = 'none'; audio_output_device = 'auto' }
        }
        # Demo purposes only: open the music patch directly without the welcome overlay.
        # Update existing demo preferences too; leave normal Bespoke preferences alone.
        $demoPreferences['show_welcome_screen'] = $false
        $demoPreferences | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $preferences
        $null = Ensure-Process 'bespoke' $bespokeExe @($preferences) @('BespokeSynth') @($bespokeExe)

        if ($Web -and !$NoWeb) { Start-NightshadeViewer }
        $hub = Get-HubState
        foreach ($session in $hub.sessions) { Write-Host "Ready: $(@($session.lanes).Count) live lanes / $($session.session)" -ForegroundColor Green }
        Write-Host "Native viewer ready. Logs: $logDirectory"
        Write-Host 'Re-running this launcher reuses these apps. Use -Status to inspect or -Stop to stop only launcher-owned processes.'
        Write-Host 'A reused Rustel keeps its current score and audio settings; -Score and -Silent apply when starting a new player.'
    } finally { Pop-Location }
} catch {
    Write-Host "Music launcher: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "Logs: $logDirectory"
    exit 1
} finally {
    if ($lock) { $lock.Dispose() }
}
