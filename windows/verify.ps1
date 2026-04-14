<#
.SYNOPSIS
Verify the split weaveback CLI works correctly in a clean Windows environment.

.DESCRIPTION
Locates or downloads `wb-tangle`, runs --version and a smoke test,
and reports any missing dependencies.
#>

$ErrorActionPreference = "Stop"

Write-Host "=== weaveback Windows verification ===" -ForegroundColor Cyan

# ── Locate binary ─────────────────────────────────────────────────────────────

$exeName  = "wb-tangle.exe"
$localExe = Join-Path $PSScriptRoot $exeName
$devExe   = Join-Path $PSScriptRoot "..\target\release\$exeName"
$mingwExe = Join-Path $PSScriptRoot "..\target\x86_64-pc-windows-gnu\release\wb-tangle.exe"

if (Test-Path $localExe) {
    $wbTangle = $localExe
    Write-Host "Using local exe: $wbTangle" -ForegroundColor Cyan
} elseif (Test-Path $devExe) {
    $wbTangle = $devExe
    Write-Host "Using dev build: $wbTangle" -ForegroundColor Cyan
} elseif (Test-Path $mingwExe) {
    $wbTangle = $mingwExe
    Write-Host "Using MinGW build: $wbTangle" -ForegroundColor Cyan
} else {
    Write-Host "Downloading latest release..." -ForegroundColor Yellow
    $url      = "https://github.com/giannifer7/weaveback/releases/latest/download/wb-tangle-mingw64.exe"
    $wbTangle = "$env:TEMP\wb-tangle.exe"
    Invoke-WebRequest -Uri $url -OutFile $wbTangle
}

# ── --version ─────────────────────────────────────────────────────────────────

Write-Host "`nRunning: wb-tangle --version" -ForegroundColor Yellow
& $wbTangle --version
if ($LASTEXITCODE -ne 0) { Write-Host "FAIL: --version returned $LASTEXITCODE" -ForegroundColor Red; exit 1 }
Write-Host "OK" -ForegroundColor Green

# ── Smoke test: expand a simple literate file ─────────────────────────────────

Write-Host "`nSmoke test: expand a literate file" -ForegroundColor Yellow
$tmp = Join-Path $env:TEMP "weaveback_verify"
New-Item -ItemType Directory -Path $tmp -Force | Out-Null

$src = Join-Path $tmp "test.md"
@"
# <[@file out.txt]>=
hello from weaveback
