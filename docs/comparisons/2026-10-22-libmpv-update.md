# libmpv 2026-07-10 → 2026-10-22 on a Win10 laptop

The July zhongfly build that 0.1 shipped with was pruned upstream, so 0.2.0
moves to the 2026-10-22 build. This run checks that the newer libmpv costs no
more than the old one and that FSR and Anime4K still compile on its libplacebo.

## Environment

- OS: Windows 10 Home Single Language 22H2, build 19045.6456, on AC power.
- CPU/GPU: AMD Ryzen 7 3700U, Radeon Vega 10 integrated graphics
  (driver 27.20.11036.3), 6 GB RAM.
- Monitor: built-in panel, 1920x1080 at 150% scaling.
- Renderer: release build. Both runs of a profile used the same `renderer.exe`;
  only the `libmpv-2.dll` next to it differed:
  - old: taken from `LimeWall-0.1.0-beta-win64.zip`, client API 2.5, mpv
    `e5486b96d7`;
  - new: `mpv-dev-lgpl-x86_64-20261022-git-488fec0789.7z`, client API 2.6.
- Sources: synthetic `testsrc2` clips, 30 fps, encoded with `h264_mf`:
  1920x1080 for Eco and Balanced, 960x540 for Max and Anime4K so that the
  upscaling shaders actually switch on.

## Method

Each run played the clip as a wallpaper, waited 8 s, then took 15 one-second
samples:

- CPU: process CPU time divided by wall time over all 8 logical CPUs, the way
  Task Manager counts it;
- GPU: the sum of the process's `GPU Engine` utilization counters (3D plus
  video decode), so the number is only good for comparing the two builds;
- GPU memory: `GPU Process Memory` dedicated plus shared.

Then the run took a screenshot through the control channel and sent `stop`.
Old and new ran back to back for each profile. Each cell is one run, so the
noise between runs is not measured.

## Results

| Profile | Source | CPU, old / new | GPU, old / new | GPU memory, old / new |
|---|---|---:|---:|---:|
| Eco (bilinear) | 1080p | 0.48% / 0.72% | 30.9% / 31.9% | 92 / 91 MB |
| Balanced (lanczos) | 1080p | 0.45% / 0.45% | 31.4% / 31.4% | 87 / 90 MB |
| Max (FSR) | 540p | 0.58% / 0.60% | 31.2% / 32.4% | 108 / 107 MB |
| Anime4K Mode A | 540p | 1.02% / 0.89% | 58.8% / 57.0% | 210 / 194 MB |

All eight runs reported `d3d11va` hardware decoding, took their screenshot and
exited with code 0. The renderer logged `FSR active (960x540 -> 1920x1080)` and
`Anime4K Mode A active` for the shader profiles with both builds.

The differences are within what one run per cell can tell apart. The new
`--hwdec-extra-frames=auto` default that landed in this range did not raise
GPU memory on this machine.

The bundled `ffmpeg.exe` moved with it (`N-127937-g23918b320`). The library
import commands ran unchanged on a 321x241 GIF: `h264_mf` produced a 320x240
mp4, the `libvpx-vp9` fallback worked too, and a 480 px preview came out of the
`-ss 1` seek.

Not covered: Windows 11, a discrete GPU, real (non-synthetic) footage and
battery power.
