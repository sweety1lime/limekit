# Changelog

## Unreleased

### Added

- **Themes: cursors and sounds.** The "Windows" screen has a "Темы" tab. Its
  cursor sets are the ones Windows ships (Windows Aero, Black and Inverted, in
  regular, large and extra large), each with a picture of a few of its
  cursors. Apply one and the cursors change at once, with no sign-out. "Без
  звуков" silences the sound of every Windows event that has one, signing in
  and unlocking included, and leaves the alarm and ringtone melodies alone.
  "Вернуть" gives back exactly what was there before LimeKit, cursors and
  sounds that were empty or not set at all included. Cursors and sounds go on
  and off independently. A theme goes through the same journal as a tweak, so
  "Вернуть всё", the `.reg` undo and "Убрать LimeKit из системы" cover it too.
  Themes are not checked on any Windows build yet, and the screen says so.
- **Theme packages, `.limetheme`.** "Добавить тему" on the "Темы" tab, a drop
  onto the window or a double click on the file adds a package of cursors and
  sounds to the list, under "Мои темы". Adding changes nothing in Windows until
  you press "Применить", and "Удалить" takes a theme that is in effect off
  first. A package is a zip with a `theme.json` and nothing but `.cur`, `.ani`,
  `.wav` and a picture. Another kind of file, a file that is not what its
  extension says, one over its size limit, or a name that climbs out of the
  folder refuses the whole package and leaves nothing on disk. Wallpaper
  packages stay `.limekit` and stay apart: a wallpaper renamed to `.limetheme`
  says it is a wallpaper, and the `.limetheme` association is written only
  after the first import and removed by "Убрать LimeKit из системы".

### Changed

- The "Твики" button in the title bar is now "Windows": tweaks and themes sit
  on one screen with two tabs. The main screen has a strip, "Настройка
  Windows", with "Твики Windows" and "Темы" buttons under the library and
  under the drop zone of an empty library, so the way in is not only the small
  title-bar button.

### For developers

- New crate `crates/themes`; `platform::cursors` reads the schemes Windows
  ships, and `platform::shell` can reload cursors and resolve `@file,-id`
  strings. The design is in `docs/research/themes.md`.
- `wpk::list_members` and `wpk::read_member` give another zip format the same
  name-checked, size-capped reading that wallpaper packages get; the theme
  library imports a package through them, file by file, instead of unpacking it.

## 0.3.1 — 36 tweaks, and two that Explorer no longer undoes

17 more tweaks, 36 in all, 33 of them checked on Windows 10 22H2. File
extensions and hidden files, two tweaks from 0.3.0, no longer turn themselves
off when the Folder Options dialog is closed.

### Added

- **17 more tweaks, 36 in all.** File Explorer: no recent files and no frequent
  folders in Quick access, no OneDrive and sync service offers, check boxes
  for selecting files, icons instead of thumbnails, the navigation pane
  expanding to the open folder, all folders in the navigation pane. Taskbar,
  for Windows 10: a separate button for every window and small buttons. Also
  no account notifications in Start, no recently opened items in jump lists,
  the language list hidden from websites, no search history on the PC, and
  the accent color on the taskbar and on title bars. A new "Уведомления" group
  holds two switches, no notifications on the lock screen and no notification
  sounds. 33 of the 36 are checked on Windows 10 22H2; the other
  three are for Windows 11 only and say they are not checked yet.

### Fixed

- **"Показывать расширения файлов" and "Показывать скрытые файлы" could be
  undone by Windows.** File Explorer keeps these settings, and two more, in
  memory as well as in the registry, and its Folder Options dialog loads that
  copy and writes it back when it closes, even on Cancel. So opening the dialog
  on the View tab quietly reverted the tweak. After a switch LimeKit now tells
  Explorer that the settings changed, and Explorer reads the registry again. If
  you turned either one on in 0.3.0, Explorer may still hold the old state:
  switch the tweak off and on once.

### Changed

- The background player shows up in Task Manager and in the file's
  Properties as "LimeKit wallpaper player" by 2fame. It used to be "renderer"
  with no publisher, in both `renderer.exe` and `limekit-daemon.exe`.

### For developers

- The tweaks for file name extensions, hidden files, check boxes and icons only
  announce `ShellState` after a switch (`notify` in the catalog).
  `docs/research/tweaks-catalog.md` explains why they do not go through
  `SHGetSetSettings`.

## 0.3.0 — tweaks you can undo, and web wallpapers that show

LimeKit gets its second module: 19 Windows tweaks, each written to a journal
before it changes anything and undone exactly, the 16 for Windows 10 checked
on 22H2. Testing the release on a laptop also turned up web wallpapers that
had never shown their page and video wallpapers lost after an Explorer
restart; both are fixed.

### Added

- **Tweaks.** A "Твики" screen next to the catalog lists the tweaks with what
  this Windows is (version, build, edition), whether each one fits it and was
  checked on this build, where its values come from, and when the change shows.
  Before anything changes it shows each value as it is and as it will be.
  Turning a tweak off undoes what LimeKit changed for it, so a value you had
  set by hand comes back; a tweak LimeKit did not turn on gets the Windows
  default. "Вернуть всё" undoes every change, and the undo can be saved as a
  .reg file that works without LimeKit. Values changed after LimeKit set them
  are left alone and named. There are 19 tweaks in six groups: File Explorer,
  the taskbar, Start, Windows tips and ads, privacy, and appearance, including
  dark mode and turning off window and menu animations. The 16 that apply to
  Windows 10 are checked on 22H2 (19045); the three for Windows 11 only are
  not checked yet and say so.
- After a tweak File Explorer reads only when it starts, the screen offers
  "Перезапустить Проводник". It goes through the Restart Manager, the way
  installers restart Explorer, so its folder windows open again, and starts
  `explorer.exe` itself if the taskbar has not come back in ten seconds.
- "Убрать LimeKit из системы" undoes the tweak changes as well, right after
  stopping the wallpapers.

### Fixed

- **Web wallpapers showed nothing.** Since 0.1 the page never loaded: wry
  serves `wallpaper://` as `http://wallpaper.localhost/` on Windows, and the
  rule that keeps a page on its own origin only allowed `wallpaper://`, so it
  turned the page itself away. The desktop got an empty dark surface. The
  rule now allows exactly the address wry uses, and a test pins it.
- **Video wallpapers went black after Explorer restarted.** The surface came
  back as a new window while mpv went on drawing into the old one. The daemon
  now notices the new window and starts the player on it.
- **The daemon could crash while Explorer restarted.** A request that reached
  the wallpaper host while it was rebuilding the desktop hit state already in
  use and aborted the process. Such a request now gets an error instead.
- The window opened taller than the screen of a 1080p laptop at 150% scale,
  with its top, and the buttons in it, under a taskbar at the top of the
  screen. It now fits the screen it opens on and starts centered there.

### For developers

- `crates/tweaks`: the engine the tweaks run on.
  Every change, to an HKCU registry value or to a system parameter set through
  `SystemParametersInfo`, is written to a JSON Lines journal before it is
  made, read back after, and a set of changes takes effect whole or not at
  all. Changes can be undone one by one, per set or all at once, without
  overwriting a value something else has changed since; the undo exports as a
  .reg file that works without LimeKit (system parameters excepted, and the
  file says so). How it works and what it does not promise:
  `docs/research/tweaks-journal.md`.
- The tweak catalog, `crates/tweaks/catalog.toml`: every tweak as data, with
  Russian and English text, a provenance label (documented, observed,
  unsupported), its on and off values, what Windows goes by while a value is
  missing, the `WM_SETTINGCHANGE` area to announce after a switch, and the
  builds, update revisions and editions it is for. LimeKit reads the build,
  UBR and `EditionID` and tells apart verified, unverified, built into
  Windows, wrong version and wrong edition. Tests check the catalog's data,
  so a bad entry fails CI: `docs/research/tweaks-catalog.md`. How to verify a
  tweak on a build: `docs/testing/manual-checks.md`.
- `platform::registry`: HKCU values as their type and bytes exactly as
  stored, with a missing value told apart from an empty one and access denied
  apart from other errors.
- `platform::parameters`: seven documented animation flags of
  `SystemParametersInfo`, read and set for the current user.
- `platform::shell`: announcing a changed setting to open windows, and
  restarting Explorer through the Restart Manager.
- The fetch scripts retry timeouts and server errors.
- The full license texts (LGPL-2.1, LGPL-3.0, Apache 2.0) are in `licenses/`
  instead of being downloaded from gnu.org at every build: it kept failing to
  answer GitHub Actions, and the texts never change. `fetch-licenses.ps1` is
  gone.

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
