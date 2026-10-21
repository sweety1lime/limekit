# Manual checks

Things the automated tests cannot cover: they need a real desktop, a GPU and
sometimes a second monitor or a laptop. Run them from the repository root on a
visible Windows desktop before a release, and on every Windows build the release
claims to support. Fetch libmpv first with `scripts/fetch-libmpv.ps1` and use
media that may legally be tested and screenshotted.

Record the Windows build (`winver`), GPU and driver, monitor setup, and whether
the renderer was a debug or release build.

## Playback and idle behavior

1. Play H.264 and HEVC 1080p files with `--quality balanced`.
2. Confirm the layer is behind clickable desktop icons and muted by default.
3. Confirm `hardware decoding` reports D3D11VA and record CPU/GPU usage.
4. Send `pause`; after two seconds confirm renderer CPU is approximately 0%.
5. Repeat with a static 4K image and confirm idle CPU is approximately 0%.

## Upscaling

1. Use the procedure in `docs/comparisons/README.md` with the same 720p frame.
2. Confirm `max` reports FSR enabled and is visibly sharper than `eco`.
3. Confirm native-or-larger content reports FSR/Anime4K skipped.
4. Run `--anime4k`, confirm all shaders load, and record GPU frame time/load.

## Display topology

Supported display modes can be listed without changing the system:

```text
cargo run -p platform --example display_mode -- list "\\.\DISPLAY1"
```

The optional `cycle` command tests a mode first, applies it temporarily, and
restores the original mode through a guard. It intentionally accepts at most 30
seconds, but still use it only on a local test machine:

```text
cargo run -p platform --example display_mode -- cycle "\\.\DISPLAY1" 1600 900 12
```

1. Keep a video playing and change the selected monitor's resolution.
2. Wait up to two seconds; confirm the surface moves/resizes and playback stays
   alive.
3. If available, disconnect and reconnect that monitor. The surface should hide
   while absent and return to the same display device when it reappears.
4. Connect a second monitor and confirm the selected monitor remains targeted.
5. Stop playback and confirm the desktop is fully restored.

## Desktop lifecycle

These are the cases most likely to break on a new Windows build
(see `docs/research/workerw.md`).

1. Turn autostart on, sign out and sign in. The wallpaper must appear without
   opening the window.
2. Restart `explorer.exe` from Task Manager while a video and a web wallpaper
   are playing. Both must come back within a few seconds.
3. Change the system wallpaper in Settings while a wallpaper is playing.
4. Press F5 on the desktop.
5. Switch between virtual desktops.
6. Open the window, run **Диагностика** (Diagnostics) and confirm desktop icons
   are reported visible.

## Politeness

See `docs/research/politeness.md` for the rules.

1. Start a fullscreen game on the wallpaper's monitor; the wallpaper pauses
   within about two seconds. Close the game; the wallpaper resumes.
2. With two monitors, a fullscreen game on one must not pause the other.
3. Lock the session (Win+L); the wallpaper pauses and resumes after unlock.
4. Let the display turn off; the wallpaper pauses and resumes when it wakes.
5. On a laptop, unplug the charger with each battery policy (`pause`, `eco`,
   `keep`) and plug it back in.
