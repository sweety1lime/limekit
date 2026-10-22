# Third-party notices — LimeKit

LimeKit (by 2fame) is distributed with the third-party components listed below.
This notice satisfies the attribution and source-availability obligations of
their licenses. A full inventory of source-only dependencies (Rust crates, npm
packages) with their permissive licenses is in `docs/third-party.md`.

The GNU license texts referenced here ship next to this file as
`lgpl-2.1.txt`, `lgpl-3.0.txt` and `gpl-3.0.txt`, and the Apache License 2.0 as
`apache-2.0.txt`. The full MIT license text is included once at the end of this
file.

---

## mpv — `libmpv-2.dll`  (LGPL-2.1-or-later)

- **Use:** the wallpaper renderer plays video through libmpv. The DLL is loaded
  at **runtime** via `libloading`; LimeKit's own code declares its own FFI and
  does not link the mpv headers. This is the "shared library mechanism" form of
  use permitted by the LGPL.
- **Composition:** this build statically links FFmpeg configured as LGPL with
  `--enable-version3` (see below). Combining mpv (LGPL-2.1-or-later) with those
  LGPL-3.0 components means the distributed `libmpv-2.dll` is conveyed under
  **LGPL-3.0-or-later**; both license texts are included.
- **Exact build:** `zhongfly/mpv-winbuild` release `2026-10-22-488fec0789`,
  artifact `mpv-dev-lgpl-x86_64-20261022-git-488fec0789.7z`, built with
  `-Dgpl=false`. The same archive is mirrored in the LimeKit release
  `deps-2026-10-22`. Reproduce with `scripts/fetch-libmpv.ps1` (SHA-256 pinned).
- **Upstream source:** <https://mpv.io> · mpv commit
  <https://github.com/mpv-player/mpv/tree/488fec0789e5026804ac8dd03fb749570416d48a>.
- **Replacing it (LGPL relinking right):** because libmpv is a standalone DLL
  loaded at runtime, you may substitute your own build of a client-API-compatible
  `libmpv-2.dll` in the install folder and LimeKit will use it — no relinking of
  LimeKit is required.

## FFmpeg — `ffmpeg.exe`  (LGPL-3.0)

- **Use:** the library import pipeline runs `ffmpeg.exe` as a **separate
  process** (GIF → mp4 conversion, JPEG previews). Nothing from FFmpeg is linked
  into LimeKit.
- **Exact build:** `zhongfly/mpv-winbuild` release `2026-10-22-488fec0789`,
  artifact `ffmpeg-lgpl-x86_64-git-23918b320.7z`, configured with
  `--enable-version3` and **without** any GPL components. Mirrored in the
  LimeKit release `deps-2026-10-22`. Reproduce with `scripts/fetch-ffmpeg.ps1`
  (SHA-256 pinned).
- **Upstream source:** <https://ffmpeg.org> · FFmpeg commit
  `23918b32004a517278ba441b9d0cb5307ced0cdb` at <https://git.ffmpeg.org/ffmpeg>.
- **Replacing it:** swap `ffmpeg.exe` for your own LGPL FFmpeg build; the import
  pipeline invokes it by path.

## Vulkan loader — `vulkan/vulkan-1.dll`  (Apache-2.0, MIT)

- **Use:** `libmpv-2.dll` and `ffmpeg.exe` import `vulkan-1.dll`, which a
  graphics driver with Vulkan support installs into System32. LimeKit loads this
  copy only when System32 has none.
- **Exact build:** LunarG Vulkan Runtime 1.4.363.0, x64, from
  `VulkanRT-X64-1.4.363.0-Components.zip`, signed by LunarG, Inc. The same
  archive is mirrored in the LimeKit release `deps-2026-10-22`. Reproduce with
  `scripts/fetch-vulkan-loader.ps1` (SHA-256 pinned).
- **Upstream source:** <https://github.com/KhronosGroup/Vulkan-Loader>, tag
  `vulkan-sdk-1.4.363.0`.
- **License:** Copyright (c) 2015-2026 The Khronos Group Inc., LunarG, Inc. and
  Valve Corporation; Apache License 2.0 (`apache-2.0.txt`) with MIT-licensed
  parts. The runtime's own notice, with the MIT texts, ships as
  `VulkanRT-License.txt` next to this file.

## AMD FidelityFX Super Resolution 1.0 — `shaders/FSR.glsl`  (MIT)

Copyright (c) 2021 Advanced Micro Devices, Inc. The MIT header is retained in the
file. Ported for mpv `glsl-shaders` (EASU + RCAS).

## Anime4K v4.0.1 (GLSL) — `shaders/anime4k/*.glsl`  (MIT / public domain)

Copyright (c) 2019-2021 bloc97 and the Anime4K contributors. MIT for
`Clamp_Highlights`, `Restore_CNN_M` and the two `Upscale_CNN` passes; public
domain for the two `AutoDownscalePre` passes. Each file retains its own license
header. Upstream: <https://github.com/bloc97/Anime4K>.

## three.js r160 — `web/viewer/*`  (MIT)

Copyright © 2010-2024 three.js authors. The `@license` header is retained in
`three.module.min.js`; the bundled `GLTFLoader.js` and `BufferGeometryUtils.js`
are covered by the same MIT license. Upstream: <https://threejs.org>.

---

## MIT License

Applies to the FSR, Anime4K (MIT portions) and three.js components above, with
the respective copyright holders named in each section.

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
