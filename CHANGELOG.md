# Changelog

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
