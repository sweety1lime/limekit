# Smoke test for a portable LimeKit folder, the one the release zip unpacks to.
# Checks that every file is in place, that the bundled ffmpeg can turn a video
# into a .limekit package, and that the daemon plays a video and a web page
# behind the desktop icons and shuts down cleanly. CI runs it on the unpacked
# zip on a clean machine, so nothing from the build tree can stand in for a
# missing file.
#
# Usage: scripts/smoke-test.ps1 -Dir dist\LimeKit
param([Parameter(Mandatory)] [string]$Dir)
$ErrorActionPreference = "Stop"
$Dir = (Resolve-Path $Dir).Path
$repoRoot = Split-Path -Parent $PSScriptRoot

$work = Join-Path ([IO.Path]::GetTempPath()) "limekit-smoke-$PID"
New-Item -ItemType Directory -Force $work | Out-Null
# Keeps the test away from the real library, wallpapers and autostart.
$env:LIMEKIT_DATA_DIR = Join-Path $work "data"

$renderer = Join-Path $Dir "renderer.exe"
$ffmpeg = Join-Path $Dir "ffmpeg.exe"

function Invoke-Tool {
    param([string]$Exe, [string[]]$Arguments, [switch]$AllowFailure)
    # The exit code decides. Under "Stop", Windows PowerShell would turn any
    # line a native program writes to stderr into an exception.
    $ErrorActionPreference = "Continue"
    $output = & $Exe @Arguments 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0 -and -not $AllowFailure) {
        throw "$(Split-Path -Leaf $Exe) $($Arguments -join ' ') failed ($LASTEXITCODE):`n$output"
    }
    $output
}

Write-Host "== files"
$required = @(
    "LimeKit.exe", "limekit-daemon.exe", "renderer.exe", "libmpv-2.dll", "ffmpeg.exe",
    "LICENSE.txt", "README.txt",
    "licenses\THIRD-PARTY-NOTICES.md", "licenses\lgpl-2.1.txt", "licenses\lgpl-3.0.txt",
    "licenses\gpl-3.0.txt", "vulkan\vulkan-1.dll", "licenses\VulkanRT-License.txt",
    "licenses\apache-2.0.txt",
    "shaders\FSR.glsl", "web\demo\index.html", "web\viewer\three.module.min.js"
)
$missing = $required | Where-Object { -not (Test-Path (Join-Path $Dir $_)) }
if ($missing) { throw "missing from the build: $($missing -join ', ')" }
$anime4k = @(Get-ChildItem (Join-Path $Dir "shaders\anime4k") -Filter "*.glsl").Count
if ($anime4k -ne 6) { throw "expected 6 Anime4K shaders, found $anime4k" }

Write-Host "== version"
$expected = (Select-String -Path (Join-Path $repoRoot "Cargo.toml") -Pattern '^version = "(.+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$reported = (Invoke-Tool $renderer @("--version")).Trim()
if ($reported -ne "renderer $expected") { throw "renderer reports '$reported', Cargo.toml says $expected" }

Write-Host "== pack a video with the bundled ffmpeg"
$clip = Join-Path $work "clip.mp4"
# ffmpeg.exe imports vulkan-1.dll; give it the bundled folder the way LimeKit
# does, for this call only, so the daemon below has to find it on its own.
$savedPath = $env:PATH
$env:PATH = "$env:PATH;$(Join-Path $Dir 'vulkan')"
# mpeg4 is FFmpeg's own encoder: no Media Foundation needed on the runner.
Invoke-Tool $ffmpeg @("-y", "-loglevel", "error", "-f", "lavfi",
    "-i", "testsrc2=size=640x360:rate=30:duration=2", "-c:v", "mpeg4", $clip) | Out-Null
$env:PATH = $savedPath
$package = Join-Path $work "smoke.limekit"
Invoke-Tool $renderer @("pack", $clip, "--name", "Smoke", "--reencode", "--out", $package) | Out-Null
$inspected = Invoke-Tool $renderer @("inspect", $package)
if ($inspected -notmatch '"id": "smoke"') { throw "inspect did not read the package back:`n$inspected" }

Write-Host "== daemon"
$endpoint = "limekit-smoke-$PID"
$daemonOut = Join-Path $work "daemon-out.txt"
$daemonErr = Join-Path $work "daemon-err.txt"
# Start-Process joins arguments with spaces and quotes nothing.
$daemon = Start-Process -FilePath $renderer -PassThru -WindowStyle Hidden `
    -ArgumentList "serve", "--endpoint", $endpoint, "--state", "`"$(Join-Path $work 'state.json')`"" `
    -RedirectStandardOutput $daemonOut -RedirectStandardError $daemonErr
# Without touching Handle now, ExitCode reads empty once the process is gone.
$null = $daemon.Handle
$ctl = { param([string[]]$Command) Invoke-Tool $renderer (@("ctl", "--endpoint", $endpoint) + $Command) }
try {
    $deadline = (Get-Date).AddSeconds(15)
    while ($true) {
        Invoke-Tool $renderer @("ctl", "--endpoint", $endpoint, "ping") -AllowFailure | Out-Null
        if ($LASTEXITCODE -eq 0) { break }
        if ($daemon.HasExited -or (Get-Date) -gt $deadline) { throw "daemon did not answer ping" }
        Start-Sleep -Milliseconds 300
    }
    Write-Host (& $ctl @("list-monitors")).Trim()

    & $ctl @("play", $clip) | Out-Null
    Start-Sleep -Seconds 3
    $status = & $ctl @("status")
    if ($status -notmatch [regex]::Escape("clip.mp4")) { throw "video is not in the status:`n$status" }
    Write-Host $status.Trim()

    # The driver's loader when System32 has one, the bundled copy otherwise.
    $expected = if (Test-Path (Join-Path $env:SystemRoot "System32\vulkan-1.dll")) { "system" } else { "bundled" }
    $loaderLine = Select-String -Path $daemonOut -Pattern "^vulkan loader: " | Select-Object -First 1
    if (-not $loaderLine -or $loaderLine.Line -notmatch "^vulkan loader: $expected") {
        throw "expected the $expected Vulkan loader, the daemon logged: $($loaderLine.Line)"
    }
    Write-Host $loaderLine.Line

    # A web wallpaper builds a WebView2 inside the host thread's message loop,
    # the path that once took the whole daemon down.
    & $ctl @("play", (Join-Path $Dir "web\demo\index.html")) | Out-Null
    Start-Sleep -Seconds 5
    if ($daemon.HasExited) { throw "daemon exited while starting a web wallpaper" }
    $status = & $ctl @("status")
    if ($status -notmatch "index\.html") { throw "web page is not in the status:`n$status" }
    Write-Host $status.Trim()

    & $ctl @("stop") | Out-Null
    & $ctl @("shutdown") | Out-Null
    if (-not $daemon.WaitForExit(15000)) { throw "daemon did not exit after shutdown" }
    if ($daemon.ExitCode -ne 0) { throw "daemon exited with code $($daemon.ExitCode)" }
    if (Select-String -Path $daemonErr -Pattern "panicked" -Quiet) { throw "daemon panicked" }
}
catch {
    Write-Host "--- daemon stdout"; Get-Content $daemonOut -ErrorAction SilentlyContinue
    Write-Host "--- daemon stderr"; Get-Content $daemonErr -ErrorAction SilentlyContinue
    throw
}
finally {
    if (-not $daemon.HasExited) { Stop-Process -Id $daemon.Id -Force }
}

Remove-Item $work -Recurse -Force
Write-Host "smoke test passed"
