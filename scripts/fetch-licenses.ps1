# Fetches the canonical license texts that the bundled runtime components
# require us to distribute: GNU texts for the LGPL libmpv and ffmpeg, Apache 2.0
# for the Vulkan loader. Kept out of git (like third_party/) and pulled from the
# authoritative source rather than transcribed. Bundled into the portable build
# by scripts/build-portable.ps1. See licenses/THIRD-PARTY-NOTICES.md.
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$dir = Join-Path $repoRoot "licenses\vendor"
New-Item -ItemType Directory -Force $dir | Out-Null

# libmpv is LGPL-2.1-or-later; the bundled build links FFmpeg (LGPL-3.0, i.e.
# additional permissions on top of GPL-3.0), so all three texts are needed.
# The Vulkan Runtime's own license file names Apache 2.0 but does not carry its
# text, which Apache 2.0 requires to travel with the binary.
# Each entry: URL and a line the genuine text starts with.
$gnu = "GNU (LESSER )?GENERAL PUBLIC LICENSE"
$texts = @{
    "lgpl-2.1.txt"   = @("https://www.gnu.org/licenses/lgpl-2.1.txt", $gnu)
    "lgpl-3.0.txt"   = @("https://www.gnu.org/licenses/lgpl-3.0.txt", $gnu)
    "gpl-3.0.txt"    = @("https://www.gnu.org/licenses/gpl-3.0.txt", $gnu)
    "apache-2.0.txt" = @("https://www.apache.org/licenses/LICENSE-2.0.txt", "Apache License")
}

foreach ($name in $texts.Keys) {
    $target = Join-Path $dir $name
    $url, $preamble = $texts[$name]
    Write-Host "downloading $url"
    curl.exe -sfL --retry 3 --retry-delay 2 -o $target $url
    if ($LASTEXITCODE -ne 0) { throw "download failed: $url" }
    # A hijacked mirror would not return the license preamble.
    if (-not (Select-String -Path $target -Pattern $preamble -Quiet)) {
        Remove-Item $target
        throw "unexpected content for $name - deleted"
    }
}

Write-Host "done; license texts in $dir"
