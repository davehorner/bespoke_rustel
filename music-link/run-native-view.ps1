$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$executable = Join-Path $workspace 'BespokeSynth/ignore/music-build/Source/BespokeSynth_artefacts/Release/BespokeSynth.exe'
$preferences = Join-Path $PSScriptRoot 'native-view-prefs.json'
if (!(Test-Path -LiteralPath $preferences)) {
    @{
        layout = (Join-Path $PSScriptRoot 'native-view-layout.json').Replace('\', '/')
        audio_input_device = 'none'
        audio_output_device = 'auto'
    } | ConvertTo-Json | Set-Content -LiteralPath $preferences
}
# Demo purposes only: skip the welcome overlay in this demo-specific preferences file.
$demoPreferences = Get-Content -LiteralPath $preferences -Raw | ConvertFrom-Json -AsHashtable
$demoPreferences['show_welcome_screen'] = $false
$demoPreferences | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $preferences
Start-Process -FilePath $executable -ArgumentList @($preferences) -WorkingDirectory (Join-Path $workspace 'BespokeSynth')
