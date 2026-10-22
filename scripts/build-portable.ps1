# Builds a portable LimeKit folder: the UI, the renderer daemon and their
# runtime dependencies laid out side by side, so LimeKit.exe runs without a
# dev server. Output: dist/LimeKit/.
#
# -Auditable builds through `cargo auditable` (cargo install cargo-auditable),
# which embeds the dependency list in each .exe so `cargo audit bin` and SBOM
# scanners can read it. Release builds in CI use it.
param([switch]$Auditable)
$ErrorActionPreference = "Stop"
# Typed: a one-element array would unroll to a plain string, and splatting a
# string passes it letter by letter.
[string[]]$cargoBuild = if ($Auditable) { "auditable", "build" } else { "build" }
$repoRoot = Split-Path -Parent $PSScriptRoot
$out = Join-Path $repoRoot "dist\LimeKit"

# Runtime binaries must be fetched first (kept out of git).
$libmpv = Join-Path $repoRoot "third_party\mpv\unpacked\libmpv-2.dll"
$ffmpeg = Join-Path $repoRoot "third_party\ffmpeg\unpacked\ffmpeg.exe"
$vulkan = Join-Path $repoRoot "third_party\vulkan\unpacked"
if (-not (Test-Path $libmpv)) { throw "run scripts/fetch-libmpv.ps1 first" }
if (-not (Test-Path $ffmpeg)) { throw "run scripts/fetch-ffmpeg.ps1 first" }
if (-not (Test-Path (Join-Path $vulkan "vulkan-1.dll"))) { throw "run scripts/fetch-vulkan-loader.ps1 first" }

# License texts must ship with the binaries above: GNU texts for libmpv and
# ffmpeg, Apache 2.0 for the Vulkan loader. Fetch them if the maintainer has not
# run scripts/fetch-licenses.ps1 yet; only warn when offline so a dev build
# still completes (a public release must include them).
$vendorLicenses = Join-Path $repoRoot "licenses\vendor"
$missingLicenses = @("lgpl-3.0.txt", "apache-2.0.txt") |
    Where-Object { -not (Test-Path (Join-Path $vendorLicenses $_)) }
if ($missingLicenses) {
    Write-Host "fetching license texts..."
    try { & (Join-Path $PSScriptRoot "fetch-licenses.ps1") }
    catch { Write-Warning "could not fetch license texts (offline?): $_ - bundling notice only" }
}

Write-Host "building renderer (release)..."
& cargo @cargoBuild -p renderer --release
if ($LASTEXITCODE -ne 0) { throw "renderer build failed" }

# The two steps `tauri build --no-bundle` runs, called directly so that cargo
# can be swapped for `cargo auditable`, which the Tauri CLI has no hook for.
Write-Host "building UI frontend..."
Push-Location (Join-Path $repoRoot "apps\ui")
& npm.cmd run build
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "UI frontend build failed" }
Pop-Location
Write-Host "building UI (release, no installer)..."
Push-Location (Join-Path $repoRoot "apps\ui\src-tauri")
& cargo @cargoBuild --bins --features tauri/custom-protocol --release
if ($LASTEXITCODE -ne 0) { Pop-Location; throw "UI build failed" }
Pop-Location

$renderer = Join-Path $repoRoot "target\release\renderer.exe"
# Windowless daemon: what autostart launches, so no console window at logon.
$daemon = Join-Path $repoRoot "target\release\limekit-daemon.exe"
$uiRelease = Join-Path $repoRoot "apps\ui\src-tauri\target\release"
$ui = Get-ChildItem $uiRelease -Filter "*.exe" |
    Where-Object { $_.Name -in @("LimeKit.exe", "ui.exe") } |
    Select-Object -First 1
if (-not $ui) { throw "UI executable not found in $uiRelease" }

if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory -Force $out | Out-Null
New-Item -ItemType Directory -Force (Join-Path $out "shaders\anime4k") | Out-Null

Copy-Item $ui.FullName (Join-Path $out "LimeKit.exe") -Force
Copy-Item $renderer (Join-Path $out "renderer.exe") -Force
Copy-Item $daemon (Join-Path $out "limekit-daemon.exe") -Force
Copy-Item $libmpv $out -Force
Copy-Item $ffmpeg $out -Force
# Its own folder, not next to libmpv: there it would shadow the graphics
# driver's loader. LimeKit loads it only when System32 has none.
New-Item -ItemType Directory -Force (Join-Path $out "vulkan") | Out-Null
Copy-Item (Join-Path $vulkan "vulkan-1.dll") (Join-Path $out "vulkan") -Force
Copy-Item (Join-Path $repoRoot "assets\shaders\FSR.glsl") (Join-Path $out "shaders") -Force
Copy-Item (Join-Path $repoRoot "assets\shaders\anime4k\*.glsl") (Join-Path $out "shaders\anime4k") -Force
# Sample web wallpaper for testing.
Copy-Item (Join-Path $repoRoot "assets\web") (Join-Path $out "web") -Recurse -Force
# Instructions for beta testers, copied verbatim (keeps its UTF-8 bytes intact).
Copy-Item (Join-Path $repoRoot "packaging\README-portable.txt") (Join-Path $out "README.txt") -Force
# LimeKit itself is GPL-3.0, and the license has to travel with the binaries.
Copy-Item (Join-Path $repoRoot "LICENSE") (Join-Path $out "LICENSE.txt") -Force

# Third-party license notices (LGPL binaries above, the Vulkan loader, MIT
# shaders/three.js).
$licensesOut = Join-Path $out "licenses"
New-Item -ItemType Directory -Force $licensesOut | Out-Null
Copy-Item (Join-Path $repoRoot "licenses\THIRD-PARTY-NOTICES.md") $licensesOut -Force
Copy-Item (Join-Path $vulkan "VulkanRT-License.txt") $licensesOut -Force
if (Test-Path $vendorLicenses) {
    Copy-Item (Join-Path $vendorLicenses "*.txt") $licensesOut -Force
} else {
    Write-Warning "GNU license texts missing - run scripts/fetch-licenses.ps1 before a public release"
}

Write-Host ""
Write-Host "portable build ready: $out"
Get-ChildItem $out -Recurse -File | ForEach-Object {
    "  {0,-22} {1,8:N0} bytes" -f $_.FullName.Substring($out.Length + 1), $_.Length
}
# The release archive, named the way the beta welcome note tells testers to
# look for it, plus SHA-256 sums for the release notes. The sums file sits next
# to the archive rather than inside it: a checksum shipped in the same download
# proves nothing. Paths are relative to dist/, so `sha256sum -c` works there.
$version = (Select-String -Path (Join-Path $repoRoot "Cargo.toml") -Pattern '^version = "(.+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$dist = Split-Path -Parent $out
$zipName = "LimeKit-$version-win64.zip"
$zip = Join-Path $dist $zipName
if (Test-Path $zip) { Remove-Item $zip -Force }
# The bsdtar that ships with Windows 10+, not Compress-Archive: Windows
# PowerShell 5.1 writes backslashes into zip entry names, which the zip format
# does not allow and unzip on other systems warns about. Full path, because Git
# for Windows can put GNU tar, which cannot write zip, earlier on PATH.
& (Join-Path $env:SystemRoot "System32\tar.exe") -a -c -f $zip -C $dist (Split-Path -Leaf $out)
if ($LASTEXITCODE -ne 0) { throw "zip failed" }

$hashed = @($zipName) + (@("LimeKit.exe", "limekit-daemon.exe", "renderer.exe") |
    ForEach-Object { "LimeKit/$_" })
$sums = foreach ($relative in $hashed) {
    $hash = (Get-FileHash -Algorithm SHA256 (Join-Path $dist $relative)).Hash.ToLower()
    "$hash  $relative"
}
# LF line endings: sha256sum -c reads a trailing CR as part of the file name.
[IO.File]::WriteAllText((Join-Path $dist "SHA256SUMS.txt"), ($sums -join "`n") + "`n")

Write-Host ""
Write-Host "release archive: $zip"
Write-Host "SHA-256 (also in dist\SHA256SUMS.txt, paste into the release notes):"
$sums | ForEach-Object { "  $_" }

Write-Host ""
Write-Host "run it: `"$out\LimeKit.exe`""
