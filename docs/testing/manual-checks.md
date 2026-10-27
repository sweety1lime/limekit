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

## Tweaks

A tweak counts as verified on a build only after this pass (see
`docs/research/tweaks-catalog.md`). Use a test account or note your own
settings first: the pass switches real settings, although each step is undone.

For every tweak the **Твики** screen offers on this build:

1. Note what the switch shows before you touch it, and check that it matches
   Windows: the same setting in Settings, Folder Options or Performance
   Options.
2. Turn the tweak on. The confirmation must list the values that change.
3. Look at the result below. Note whether it showed at once, only after
   **Перезапустить Проводник**, or only after signing out, and compare with
   the chip on the card.
4. Turn it off. The setting must be back to what step 1 showed, in Windows'
   own settings as well.
5. If it all held, add the build number to `verified` in
   `crates/tweaks/catalog.toml`, and correct `takes_effect` if step 3 said
   otherwise.

What to look at:

| Tweak | Where it shows |
|---|---|
| `explorer.file_extensions` | a folder with `.txt` files |
| `explorer.hidden_files` | `%USERPROFILE%` shows `AppData` |
| `explorer.open_to_this_pc` | Win+E opens on the drives |
| `taskbar.hide_task_view` | the Task View button on the taskbar |
| `taskbar.align_left` | Start and the icons move left (Windows 11) |
| `taskbar.search_icon` | the search box turns into an icon |
| `start.hide_recommendations` | Settings → Personalization → Start (Windows 11) |
| `suggestions.start_menu` | Settings → Personalization → Start, "Show suggestions occasionally" (Windows 10) |
| `suggestions.tips` | Settings → System → Notifications, "Get tips…" |
| `suggestions.settings_app` | Settings → Privacy → General, "Show me suggested content" |
| `suggestions.welcome` | Settings → System → Notifications, "Show me the Windows welcome experience…" |
| `suggestions.finish_setup` | Settings → System → Notifications, "Suggest ways I can finish setting up…" |
| `suggestions.lock_screen` | Settings → Personalization → Lock screen, "Get fun facts, tips…" |
| `privacy.advertising_id` | Settings → Privacy → General, the advertising ID switch |
| `privacy.app_launch_tracking` | Settings → Privacy → General, "Let Windows track app launches…" |
| `appearance.dark_apps` | Settings and File Explorer turn dark |
| `appearance.dark_system` | the taskbar and Start turn dark (Windows 11) |
| `appearance.no_transparency` | the taskbar stops being see-through |
| `appearance.no_animations` | minimizing a window and opening a menu; Performance Options boxes |

Then, once per build:

1. Turn several tweaks on, press **Вернуть всё** and confirm everything is back.
2. Turn a tweak on, change the same setting in Windows, and confirm the screen
   names it as changed and **Вернуть всё** leaves it alone.
3. Save **Откат в .reg**, read it, and confirm it lists the values and names
   the system parameters it cannot restore.
4. Press **Перезапустить Проводник** while a video wallpaper plays. The
   taskbar must come back within ten seconds and the wallpaper with it.

## Politeness

See `docs/research/politeness.md` for the rules.

1. Start a fullscreen game on the wallpaper's monitor; the wallpaper pauses
   within about two seconds. Close the game; the wallpaper resumes.
2. With two monitors, a fullscreen game on one must not pause the other.
3. Lock the session (Win+L); the wallpaper pauses and resumes after unlock.
4. Let the display turn off; the wallpaper pauses and resumes when it wakes.
5. On a laptop, unplug the charger with each battery policy (`pause`, `eco`,
   `keep`) and plug it back in.
