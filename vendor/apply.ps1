#requires -Version 7.0
[CmdletBinding()]
param([switch]$Initialize)
$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'manifest.json') -Raw | ConvertFrom-Json
if ($manifest.version -ne 1) { throw 'Unsupported vendor manifest version.' }
if ($Initialize) {
    & git -C $workspace submodule update --init --recursive
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the published upstream submodules.' }
}
$pending = @()
foreach ($project in $manifest.projects) {
    $repository = Join-Path $workspace $project.name
    $head = & git -C $repository rev-parse HEAD
    if ($LASTEXITCODE -ne 0 -or $head -ne $project.base) {
        throw "$($project.name) is not at its pinned upstream base. Initialize its submodule before applying vendor changes."
    }
    if (!$project.patch_sha256) { continue }
    $patch = Join-Path $PSScriptRoot "$($project.name)/changes.patch"
    if ((Get-FileHash -LiteralPath $patch -Algorithm SHA256).Hash.ToLowerInvariant() -ne $project.patch_sha256) {
        throw "Vendor patch checksum mismatch: $($project.name)"
    }
    foreach ($file in $project.files) {
        if ($file.deleted) { continue }
        $copy = Join-Path $PSScriptRoot "$($project.name)/files/$($file.path)"
        if ((Get-FileHash -LiteralPath $copy -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) {
            throw "Vendor source checksum mismatch: $($project.name)/$($file.path)"
        }
    }
    & git -C $repository apply --reverse --check $patch 2>$null
    if ($LASTEXITCODE -eq 0) {
        Write-Host "$($project.name): vendor changes already present"
        continue
    }
    & git -C $repository apply --check $patch
    if ($LASTEXITCODE -ne 0) {
        throw "$($project.name) has conflicting local edits. Nothing has been overwritten; review its diff first."
    }
    $pending += @{ repository = $repository; patch = $patch; name = $project.name }
}
# Preflight every repository before modifying any working source.
foreach ($project in $pending) {
    & git -C $project.repository apply --whitespace=nowarn $project.patch
    if ($LASTEXITCODE -ne 0) { throw "Could not apply vendor changes to $($project.name)." }
    Write-Host "$($project.name): applied vendor changes"
}
Write-Host 'Workspace source is ready. Build instructions: music-link/README.md'
