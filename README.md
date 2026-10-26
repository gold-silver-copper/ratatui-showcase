# ratatui-showcase

A fullscreen [ratatui](https://ratatui.rs) demo screen for big monitors and
projectors. The ratatui logo and the "ratatui" wordmark take turns, scaled to
fill any terminal while keeping their proportions, with
[tachyonfx](https://github.com/ratatui/tachyonfx) transitions in the style of
[Omarchy](https://omarchy.org)'s screensaver.

```sh
cargo run --release
cargo run --release -- --hold 10 --glyphs sextants
```

| Key | Action |
| --- | --- |
| `Space`, `→`, `Enter` | Next scene |
| `t` | Next colour theme |
| `s` | Next shader |
| `b` | Next background |
| `l` | Next look |
| `i` | Show or hide the caption |
| `q`, `Esc`, `Ctrl-C` | Quit |

| Option | Meaning |
| --- | --- |
| `--hold SECONDS` | How long each scene stays (default 7) |
| `--fps N` | Frames per second (default 60) |
| `--glyphs blocks\|sextants\|full` | Block characters your terminal draws well. `full` (the default) adds octants and diagonal wedges, which foot, kitty, ghostty and wezterm draw themselves; use `sextants` or `blocks` elsewhere. |

## What it shows

- **Subjects:** the ratatui mascot, the pixel-art rat from `assets/logo.svg`,
  the `RatatuiLogo` block lettering, the classic 8x8 font, the word set in a
  random font installed on the system, and logo and wordmark together.
- **Styles:** solid, outlined, extruded 3D lettering, or reflected.
- **Themes:** Classic, Aurora, Synthwave, Ember, Ocean, Candy, Phosphor, Prism,
  Sunset, Glacier and Toxic.
- **Shaders:** flow, plasma, pulse, sheen, rainbow, scanline, chrome, holo,
  fire and stripes, with a soft glow around the art.
- **Backgrounds:** a vignette, twinkling stars, a synthwave grid, aurora
  curtains, or slow sun rays.
- **Motion while holding:** still, floating, waving, breathing or swaying.
- **Looks:** smooth block glyphs, chunky pixels, braille dots, ASCII, or an
  LED sign.

## Transitions

Most scene changes are drawn at sub-cell resolution across the whole screen:
radial, diamond, clock, spiral, diagonal, blinds and checkerboard wipes, burn,
zoom, swirl, ripple, mosaic, slices, glitch, a CRT switching off and on, and
particle effects where the old art's pixels morph, swarm, scatter, pour, spiral
through a vortex or crumble into the new art. When the look changes, a
[tachyonfx](https://github.com/ratatui/tachyonfx) transition plays instead:
dissolves, fades, sweeps, slides, evolving symbols, and terminaltexteffects-style
decrypt, matrix rain and light beams.

## How it scales

Art is rasterized at 4x8 samples per cell, at the exact size that fills the
terminal; pixel art scales by whole numbers so every pixel stays the same size.
Each cell on an edge gets the block glyph whose shape best matches its samples
(halves, quadrants, eighths, sextants, octants or diagonal wedges), with
colours blended in linear light, so edges stay crisp and curves smooth. The
terminal's pixel size, where it reports one, corrects for the shape of its
cells, so the art is never stretched.

## How it renders

The CPU lays each scene out (fonts, bitmaps, glow, particle positions) and the
GPU draws every frame: a [Bevy](https://bevyengine.org) compute shader
(`src/render.wgsl`) samples the art, runs the shaders, backgrounds and
transitions, splats particles, matches glyphs and writes one glyph and two
colours per cell, which are read back for ratatui to print. Bevy runs headless,
without a window, so a GPU with Vulkan, Metal or DirectX 12 drivers is needed.

## Credits

The mascot and the `RatatuiLogo` lettering come from `ratatui-widgets`, and the
rat from ratatui's `assets/logo.svg`, both MIT licensed by the ratatui
developers. The octant and wedge shapes in `src/glyph_table.rs` were generated
from foot's `box-drawing.c` (MIT) by `tools/gen_glyphs.py`.
