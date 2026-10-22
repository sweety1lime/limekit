# Downloads the pinned LGPL ffmpeg build used by the library import pipeline
# (GIF -> mp4, previews). Source and licensing: docs/third-party.md.
param(
    [string]$Tag = "2026-10-22-488fec0789",
    [string]$Mirror = "deps-2026-10-22",
    [string]$Asset = "ffmpeg-lgpl-x86_64-git-23918b320.7z",
    [string]$Sha256 = "B36232825C35ED91D84624C437FEF27259D8F156BE7C6F97C6E70A573B3AC01E"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$dir = Join-Path $repoRoot "third_party\ffmpeg"
New-Item -ItemType Directory -Force $dir | Out-Null
$archive = Join-Path $dir $Asset

if (-not (Test-Path $archive)) {
    # Mirrored for the same reason as libmpv (see fetch-libmpv.ps1).
    $urls = "https://github.com/sweety1lime/limekit/releases/download/$Mirror/$Asset",
        "https://github.com/zhongfly/mpv-winbuild/releases/download/$Tag/$Asset"
    foreach ($url in $urls) {
        Write-Host "downloading $url"
        curl.exe -sfL -o $archive $url
        if ($LASTEXITCODE -eq 0) { break }
    }
    if ($LASTEXITCODE -ne 0) { throw "download failed from every source: $($urls -join ', ')" }
}

$hash = (Get-FileHash $archive -Algorithm SHA256).Hash
if ($hash -ne $Sha256) {
    Remove-Item $archive
    throw "SHA-256 mismatch: expected $Sha256, got $hash - archive deleted, re-run to retry"
}
Write-Host "sha256 OK"

$unpacked = Join-Path $dir "unpacked"
$sevenZip = @("$env:ProgramFiles\7-Zip\7z.exe", "${env:ProgramFiles(x86)}\7-Zip\7z.exe") |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if ($sevenZip) {
    & $sevenZip x -y -o"$unpacked" $archive | Out-Null
} else {
    New-Item -ItemType Directory -Force $unpacked | Out-Null
    tar -xf $archive -C $unpacked
    if ($LASTEXITCODE -ne 0) { throw "extraction failed: install 7-Zip or a tar with 7z support" }
}

$ffmpeg = Join-Path $unpacked "ffmpeg.exe"
if (-not (Test-Path $ffmpeg)) { throw "ffmpeg.exe not found in archive" }
Write-Host "done; ffmpeg at $ffmpeg"
