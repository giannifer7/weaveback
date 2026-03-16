<#
.SYNOPSIS
Verify azadi works correctly in a clean Windows environment.

.DESCRIPTION
Locates or downloads the azadi binary, runs --version and a smoke test,
and reports any missing dependencies.
#>

$ErrorActionPreference = "Stop"

Write-Host "=== azadi Windows verification ===" -ForegroundColor Cyan

# ── Locate binary ─────────────────────────────────────────────────────────────

$exeName  = "azadi.exe"
$localExe = Join-Path $PSScriptRoot $exeName
$devExe   = Join-Path $PSScriptRoot "..\target\release\$exeName"
$mingwExe = Join-Path $PSScriptRoot "..\target\release\azadi-mingw64.exe"

if (Test-Path $localExe) {
    $azadi = $localExe
    Write-Host "Using local exe: $azadi" -ForegroundColor Cyan
} elseif (Test-Path $devExe) {
    $azadi = $devExe
    Write-Host "Using dev build: $azadi" -ForegroundColor Cyan
} elseif (Test-Path $mingwExe) {
    $azadi = $mingwExe
    Write-Host "Using MinGW build: $azadi" -ForegroundColor Cyan
} else {
    Write-Host "Downloading latest release..." -ForegroundColor Yellow
    $url     = "https://github.com/giannifer7/azadi/releases/latest/download/azadi.exe"
    $azadi   = "$env:TEMP\azadi.exe"
    Invoke-WebRequest -Uri $url -OutFile $azadi
}

# ── --version ─────────────────────────────────────────────────────────────────

Write-Host "`nRunning: azadi --version" -ForegroundColor Yellow
& $azadi --version
if ($LASTEXITCODE -ne 0) { Write-Host "FAIL: --version returned $LASTEXITCODE" -ForegroundColor Red; exit 1 }
Write-Host "OK" -ForegroundColor Green

# ── Smoke test: expand a simple literate file ─────────────────────────────────

Write-Host "`nSmoke test: expand a literate file" -ForegroundColor Yellow
$tmp = Join-Path $env:TEMP "azadi_verify"
New-Item -ItemType Directory -Path $tmp -Force | Out-Null

$src = Join-Path $tmp "test.md"
@"
# <[@file out.txt]>=
hello from azadi
# @
"@ | Set-Content $src

& $azadi $src --gen $tmp
if ($LASTEXITCODE -ne 0) { Write-Host "FAIL: expansion returned $LASTEXITCODE" -ForegroundColor Red; exit 1 }

$out = Join-Path $tmp "out.txt"
if (-not (Test-Path $out)) { Write-Host "FAIL: out.txt not created" -ForegroundColor Red; exit 1 }

$content = Get-Content $out -Raw
if ($content.Trim() -ne "hello from azadi") {
    Write-Host "FAIL: unexpected output: $content" -ForegroundColor Red; exit 1
}
Write-Host "OK" -ForegroundColor Green

# ── Summary ───────────────────────────────────────────────────────────────────

Write-Host "`nAll checks passed." -ForegroundColor Green
