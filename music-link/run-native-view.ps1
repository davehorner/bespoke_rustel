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
Start-Process -FilePath $executable -ArgumentList @($preferences) -WorkingDirectory (Join-Path $workspace 'BespokeSynth')
