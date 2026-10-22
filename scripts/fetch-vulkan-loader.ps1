# Downloads the pinned Vulkan Runtime components from LunarG and keeps the
# 64-bit Khronos loader (vulkan-1.dll, signed by LunarG) with its license. The
# portable build ships it for machines whose graphics driver has none; see
# crates/platform/src/vulkan.rs. Source and licensing: docs/third-party.md.
param(
    [string]$Version = "1.4.363.0",
    [string]$Mirror = "deps-2026-10-22",
    [string]$Sha256 = "A25A927AA8B9F0371048F1861CF88AC3B9BC9B1FB332C42D897C8AB32695769A"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$dir = Join-Path $repoRoot "third_party\vulkan"
New-Item -ItemType Directory -Force $dir | Out-Null
$asset = "VulkanRT-X64-$Version-Components.zip"
$archive = Join-Path $dir $asset

if (-not (Test-Path $archive)) {
    # Mirrored next to libmpv and ffmpeg, so a build depends on one host fewer.
    $urls = "https://github.com/sweety1lime/limekit/releases/download/$Mirror/$asset",
        "https://sdk.lunarg.com/sdk/download/$Version/windows/$asset"
    foreach ($url in $urls) {
        Write-Host "downloading $url"
        curl.exe -sfL --retry 3 --retry-delay 2 -o $archive $url
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
New-Item -ItemType Directory -Force $unpacked | Out-Null
$root = "VulkanRT-X64-$Version-Components"
# The System32 bsdtar reads zip and can pick single entries out of it.
& (Join-Path $env:SystemRoot "System32\tar.exe") -x -f $archive -C $unpacked --strip-components 1 `
    "$root/x64/vulkan-1.dll" "$root/VulkanRT-License.txt"
if ($LASTEXITCODE -ne 0) { throw "extraction failed" }
Move-Item -Force (Join-Path $unpacked "x64\vulkan-1.dll") (Join-Path $unpacked "vulkan-1.dll")
Remove-Item (Join-Path $unpacked "x64")
Write-Host "done; loader at $(Join-Path $unpacked 'vulkan-1.dll')"
