<#
.SYNOPSIS
Verify weaveback works correctly in a clean Windows environment.

.DESCRIPTION
Locates or downloads the weaveback binary, runs --version and a smoke test,
and reports any missing dependencies.
#>

$ErrorActionPreference = "Stop"

Write-Host "=== weaveback Windows verification ===" -ForegroundColor Cyan

# ── Locate binary ─────────────────────────────────────────────────────────────

$exeName  = "weaveback.exe"
$localExe = Join-Path $PSScriptRoot $exeName
$devExe   = Join-Path $PSScriptRoot "..\target\release\$exeName"
$mingwExe = Join-Path $PSScriptRoot "..\target\release\weaveback-mingw64.exe"

if (Test-Path $localExe) {
    $weaveback = $localExe
    Write-Host "Using local exe: $weaveback" -ForegroundColor Cyan
} elseif (Test-Path $devExe) {
    $weaveback = $devExe
    Write-Host "Using dev build: $weaveback" -ForegroundColor Cyan
} elseif (Test-Path $mingwExe) {
    $weaveback = $mingwExe
    Write-Host "Using MinGW build: $weaveback" -ForegroundColor Cyan
} else {
    Write-Host "Downloading latest release..." -ForegroundColor Yellow
    $url     = "https://github.com/giannifer7/weaveback/releases/latest/download/weaveback.exe"
    $weaveback   = "$env:TEMP\weaveback.exe"
    Invoke-WebRequest -Uri $url -OutFile $weaveback
}

# ── --version ─────────────────────────────────────────────────────────────────

Write-Host "`nRunning: weaveback --version" -ForegroundColor Yellow
& $weaveback --version
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
# @
"@ | Set-Content $src

& $weaveback $src --gen $tmp
if ($LASTEXITCODE -ne 0) { Write-Host "FAIL: expansion returned $LASTEXITCODE" -ForegroundColor Red; exit 1 }

$out = Join-Path $tmp "out.txt"
if (-not (Test-Path $out)) { Write-Host "FAIL: out.txt not created" -ForegroundColor Red; exit 1 }

$content = Get-Content $out -Raw
if ($content.Trim() -ne "hello from weaveback") {
    Write-Host "FAIL: unexpected output: $content" -ForegroundColor Red; exit 1
}
Write-Host "OK" -ForegroundColor Green

# ── Summary ───────────────────────────────────────────────────────────────────

Write-Host "`nAll checks passed." -ForegroundColor Green
