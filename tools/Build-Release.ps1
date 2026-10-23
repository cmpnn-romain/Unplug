# Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
<#
.SYNOPSIS
    One command, one exe: runs the gates and builds release\Unplug-<version>.exe, the single portable program.

.DESCRIPTION
    The version comes from the VERSION file and from nowhere else: it is stamped into the file name and the exe's version resource.
    The exe asks Windows for administrator rights when it starts (turning a USB device off needs them).
#>
param([switch]$SkipGates)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$Version = (Get-Content (Join-Path $root "VERSION") -TotalCount 1).Trim()
if ($Version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
    throw "VERSION must be strict SemVer MAJOR.MINOR.PATCH (e.g. 0.1.4), got: '$Version'. See docs\VERSIONING.md"
}

function Invoke-Cargo {
    param([string[]]$CargoArgs)
    # cargo prints its progress on stderr, which PowerShell would turn into a terminating error under "Stop"
    $previous = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & cargo @CargoArgs 2>&1 | ForEach-Object { Write-Output "$_" }
        $exit = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previous }
    if ($exit -ne 0) { throw "cargo $($CargoArgs -join ' ') failed" }
}

Push-Location (Join-Path $root "rust")
try {
    if (-not $SkipGates) {
        Write-Output "Gates: fmt, clippy, tests ..."
        Invoke-Cargo @("fmt", "--check")
        Invoke-Cargo @("clippy", "--all-targets", "--locked", "--", "-D", "warnings")
        Invoke-Cargo @("test", "--workspace", "--locked")
    }
    Write-Output "Building (cargo build --release) ..."
    Invoke-Cargo @("build", "--release", "--locked")
} finally { Pop-Location }

$built = Join-Path $root "rust\target\release\unplug.exe"
if (-not (Test-Path $built)) { throw "expected build output not found: $built" }
$out = Join-Path $root "release"
New-Item -ItemType Directory -Path $out -Force | Out-Null
$exe = Join-Path $out "Unplug-$Version.exe"
Copy-Item $built $exe -Force
$hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output "release: $exe"
Write-Output "sha256:  $hash"
