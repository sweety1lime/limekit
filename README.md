# LimeKit

**English** · [Русский](README.ru.md)

A Windows customization kit you can undo.

I started this as a live wallpaper app called LimeWall. The wallpapers worked out
well and I use them every day, but somewhere along the way I realised the picture
on my desktop wasn't the only thing I wanted to change about Windows. So LimeWall
became LimeKit: same engine underneath, but now it's a kit that will grow.

Wallpapers are the first module and the only finished one. Everything else is in
the plans below, and I've written them honestly — what's done is done, what
isn't, isn't.

<!-- GIF GOES HERE: live wallpaper behind the icons with the app window beside
     it. Drop the file in docs/media/ and replace this comment with
     ![LimeKit](docs/media/limekit.gif) -->

## What works today

**Live wallpapers** behind the desktop icons — video, GIF, images, HTML pages
and glTF 3D models.

- Hardware decoding through libmpv and d3d11va. On my machine 1080p H.264 sits
  between 1.5% and 5% CPU, and exactly 0% when paused.
- GPU upscaling when the source is smaller than the monitor: lanczos, FSR, or
  Anime4K for drawn content. If the source already matches the monitor, no
  shader runs — there's no reason to heat the GPU for nothing.
- Per-monitor quality, volume and playlist settings. Playlists keep rotating
  after you close the window.
- Wallpapers pause themselves when they shouldn't be running: a fullscreen game
  in focus, a locked screen, the display off, the laptop on battery. I tested it
  against a real game — the pause lands within a couple of seconds and the CPU
  drops to zero. A separate watchdog also stops any wallpaper that runs the CPU
  hot for too long.
- `.limekit` packages — export anything from your library as one file, install
  someone else's with a double-click. Older `.wpk` files from 0.1 still open.
- A community catalog inside the app; downloads are checksum-verified.

## Where this is going

In order, not by importance:

1. **System tweaks** — toggles for the taskbar, File Explorer, appearance and
   privacy. Each one shows exactly what it will change before it does, and
   reverts in one click. The whole change-journal design exists for this.
2. **Themes** — cursors, sounds, accent colours, folder icons. Build a set,
   share it as a file, put everything back the way it was.
3. **Widgets and a dock** — a clock, a system monitor, notes on the desktop; a
   dock of my own through the documented AppBar API, without touching the
   system taskbar.

I'm not promising dates: this is spare-time work, around ten hours a week. But
that's the order, and it won't change quietly — watch the releases.

## What LimeKit does not do

This isn't a list of shortcomings. These are deliberate choices, and most
Windows customization tools break systems through exactly what's absent here.

- **No code injection into system processes.** Not `explorer.exe`, not
  `dwm.exe`, nowhere. That's why there are no anti-cheat bans, no black screens
  after a Windows update, and no Explorer crash loops.
- **No patching of system files.** `System32` stays the way Microsoft shipped
  it. SFC and DISM will find nothing to complain about.
- **Nothing gets weakened.** Defender, UAC, SmartScreen, Windows Update —
  none of my business. Any "optimizer" that touches those makes you less safe.
- **No administrator rights.** Everything lives in your own profile:
  `%APPDATA%` and `HKCU`. Other users of the machine never see LimeKit.
- **No code fetched from the network.** An update is a release you download
  yourself, not a script that arrives at runtime.

One honest exception: a web wallpaper is a real HTML page and JavaScript does
run in it. So it gets a strict CSP that closes off network access, and
installing such a package asks for confirmation first. How that works, and what
I deliberately don't try to detect, is in the
[security model](docs/research/security-model.md).

## Install

Windows 10 or 11, 64-bit. Nothing else to install — it's all in the archive.

1. Download the archive from
   [Releases](https://github.com/sweety1lime/limekit/releases).
2. Unpack it anywhere.
3. Run `LimeKit.exe`.

Nothing is written into the system until you turn on autostart yourself with the
checkbox in settings. To uninstall, delete the folder.

**About the Windows warning.** Releases aren't signed with a certificate yet, so
the first launch shows "Windows protected your PC". That's not a sign of a virus
— it's the absence of a paid certificate, which costs money and isn't available
everywhere. I publish the SHA-256 of every file in the release notes, so you can
check you downloaded my build. When signing happens, I'll say so in a release.

**Upgrading from LimeWall 0.1.** Quit the old version from the tray first,
otherwise it keeps holding its files open. On first launch LimeKit moves your
library and settings from `%APPDATA%\LimeWall` to `%APPDATA%\LimeKit` by itself
— nothing to redo.

## How it's built

If you care about what's behind the button, I keep notes as I go, including the
options I rejected and why:

- [Security model](docs/research/security-model.md) — why I bet on containment
  instead of trying to detect hostile content.
- [The layer behind the icons](docs/research/workerw.md) — how you draw behind
  desktop icons at all.
- [Renderer isolation](docs/research/renderer-sandbox.md) — why a full
  AppContainer didn't fit, and what I do instead.
- [Community catalog](docs/research/workshop.md) — moderation, signing,
  revocation.
- [PLAN.md](PLAN.md) — the roadmap with acceptance criteria per phase.

The architecture in short: two processes. A long-lived daemon owns the wallpaper
surfaces and playback; the Tauri window is control only, and closing it doesn't
stop the wallpapers. They talk over local IPC. Everything platform-specific sits
behind the `WallpaperHost` trait in `crates/platform`.

## Build from source

You'll need Rust and Node. First, once — the playback runtimes (LGPL builds) and
their license texts; details in [docs/third-party.md](docs/third-party.md):

```
scripts\fetch-libmpv.ps1
scripts\fetch-ffmpeg.ps1
scripts\fetch-licenses.ps1
```

Daemon and CLI:

```
cargo run -p renderer -- play video.mp4 --monitor 0 --quality max
cargo run -p renderer -- serve
cargo run -p renderer -- ctl play video.mp4 --monitor 0 --quality max
cargo run -p renderer -- ctl pause | resume | volume 30 | status | shutdown
```

Control window:

```
cd apps/ui
npm install
npm run tauri dev
```

Windows PowerShell 5 has no `&&`, so run the commands one per line. The first
`npm run tauri dev` compiles the whole Tauri stack and can think for several
minutes.

`tauri dev` serves the frontend from a dev server and can't exercise the real
lifecycle: closing the window, reopening from the tray, autostart, a reboot. For
that, build the portable folder — every binary side by side, nothing required:

```
scripts\build-portable.ps1
```

The result lands in `dist/LimeKit/`; run `dist\LimeKit\LimeKit.exe`.

## Licensing

LimeKit's own code is [GPL-3.0](LICENSE).

libmpv and ffmpeg are used as **LGPL** builds, dynamically linked. The FSR and
Anime4K shaders are MIT and BSD. Pinned versions and sources are listed in
[docs/third-party.md](docs/third-party.md) and
[licenses/THIRD-PARTY-NOTICES.md](licenses/THIRD-PARTY-NOTICES.md).

## Bugs and ideas

Found something broken?
[Open an issue](https://github.com/sweety1lime/limekit/issues). The bug template
asks for the diagnostics report from the app (there's a button in settings) and
your Windows version — without those I usually can't reproduce the problem.
