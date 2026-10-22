# Changelog

## 0.2.1 — no Vulkan driver needed, and a clean way out

Wallpapers now play on machines whose graphics driver has no Vulkan, LimeKit
can remove everything it put outside its folder, and the release is the first
one built and attested by GitHub Actions.

### Added

- **The release is built by GitHub Actions and attested.**
  `gh attestation verify LimeKit-0.2.1-win64.zip -R sweety1lime/limekit`
  shows which workflow and commit produced the zip, and works on each of the
  three programs inside it too. An SPDX SBOM is attached to the release.
- **Settings → "Убрать LimeKit из системы".** It stops the wallpapers, removes
  autostart and the `.limekit` association (only our part of `.wpk`, if another
  program owns it), and optionally deletes the library, settings and the
  window's WebView2 data, then closes. Until now deleting the folder left the
  association and the autostart entry behind, pointing at nothing, although
  the README said deleting the folder was enough. The same thing runs as
  `LimeKit.exe --cleanup [--remove-data]`, which an uninstaller can call.

### Fixed

- **Video and image wallpapers, GIF import and previews work without a Vulkan
  driver.** The bundled libmpv and ffmpeg import `vulkan-1.dll`, which only a
  graphics driver with Vulkan support installs, so old Intel HD graphics,
  virtual machines and the Microsoft Basic Display Adapter had nothing to play
  with, since 0.1. LimeKit now carries the Khronos loader, signed by LunarG, in
  a `vulkan` folder and uses it only when System32 has none, so a driver's own
  loader still wins.
- When libmpv cannot be loaded although it is in place, the error says which
  library is missing instead of telling you to run a developer script.
- Diagnostics show which Vulkan loader is in use.

### For developers

- `scripts/fetch-vulkan-loader.ps1` fetches the loader, pinned by hash and
  mirrored with libmpv; `build-portable.ps1` needs it. `fetch-licenses.ps1`
  adds the Apache 2.0 text the loader's license requires.
- GitHub Actions (`.github/workflows/build.yml`) checks every push: format,
  clippy, tests, the type check, the portable build, and
  `scripts/smoke-test.ps1` on the unpacked zip on Windows Server 2022 and 2025.
  A `v*` tag builds without caches, attests the zip and the programs in it,
  attests an SPDX SBOM, and opens a draft release.
- `scripts/build-portable.ps1 -Auditable` builds through `cargo auditable`, so
  `cargo audit bin` can check the shipped programs. It now runs the frontend
  build and cargo itself instead of `tauri build`, which has no way to swap
  cargo out.
- The zip is written by the bsdtar that ships with Windows, with forward
  slashes in entry names; `Compress-Archive` in Windows PowerShell 5.1 wrote
  backslashes.
- `rust-toolchain.toml` pins Rust 1.95.0 for local builds and CI alike.

## 0.2.0 — LimeWall becomes LimeKit

The wallpaper engine is now the first module of a bigger kit for customizing
Windows, so it got a name that fits. Everything you had carries over.

### Changed

- **New name: LimeKit**, published as 2fame. The programs are `LimeKit.exe`,
  `limekit-daemon.exe` and `renderer.exe`, and data lives in
  `%APPDATA%\LimeKit`.
- **Packages are `.limekit` files** and show the LimeKit icon in Explorer.
  `.wpk` files from 0.1 still open, double-click included.
- **The code is licensed under GPL-3.0.** It was public before, but without a
  license, which legally meant nobody could do anything with it.
- **Newer libmpv and FFmpeg**: the 2026-10-22 builds (mpv `488fec0789`, client
  API 2.6; FFmpeg `N-127937-g23918b320`). On a Vega 10 laptop CPU, GPU load and
  GPU memory stayed where they were in every quality profile; the numbers are
  in `docs/comparisons/2026-10-22-libmpv-update.md`.

### Fixed

- **HTML and 3D wallpapers took down the background process** whenever
  WebView2 needed more than a second to start, which on a slow laptop on
  battery was every time. The wallpaper never appeared and the app lost its
  connection. 0.1 had the same bug; it went unnoticed on a fast desktop.

### Upgrading from LimeWall 0.1

Run LimeKit and it takes over by itself:

- it asks the old daemon to quit, so two of them don't end up fighting over
  the desktop;
- it moves your library, applied wallpapers and settings over from
  `%APPDATA%\LimeWall`, and if the old daemon was stuck and the move failed,
  it finishes the move on the next start instead of leaving your library
  behind;
- it carries autostart over if you had it on, and removes the old entry;
- it claims `.wpk` for double-click only if no other program owns that
  extension; otherwise it just adds itself to "Open with".

### For developers

- `LIMEKIT_DATA_DIR` points the data directory somewhere else and switches
  migration off. The tests use it, so a test run never moves a real library
  or stops a running daemon.
- File association tests run against a private registry key instead of the
  real `HKCU\Software\Classes`.
- `scripts/build-portable.ps1` now also produces
  `dist/LimeKit-<version>-win64.zip` and `dist/SHA256SUMS.txt`, and puts
  `LICENSE.txt` in the folder.
- zhongfly prunes its releases after about a month, and the build 0.1 used is
  already gone. `fetch-libmpv.ps1` and `fetch-ffmpeg.ps1` now download from
  this repository's `deps-2026-10-22` release first and from zhongfly second,
  under the same pinned hash. A failed download now says so instead of
  reporting a hash mismatch on a 404 page.
- `renderer pack` writes `<id>.limekit` instead of `<id>.wpk`.

## 0.1.0-beta

The first beta, under the name LimeWall: video, GIF, image, HTML and glTF
wallpapers behind the desktop icons, hardware decoding through libmpv, GPU
upscaling with FSR or Anime4K, per-monitor settings and playlists, automatic
pause behind fullscreen games, on battery and on the lock screen, a tray icon,
autostart, `.wpk` packages and the community catalog.
