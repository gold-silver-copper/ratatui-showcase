# ratatui-showcase

A fullscreen [ratatui](https://ratatui.rs) demo screen for big monitors and
projectors. The ratatui logo and the "ratatui" wordmark take turns, scaled to
fill any terminal while keeping their proportions, with
[tachyonfx](https://github.com/ratatui/tachyonfx) transitions in the style of
[Omarchy](https://omarchy.org)'s screensaver.

```sh
cargo run --release
cargo run --release -- --hold 10 --glyphs sextants
cargo run --release -- --watch     # edit src/shaders and watch it change
cargo run --release -- --profile   # GPU time per phase in the caption
```

| Key | Action |
| --- | --- |
| `Space`, `→`, `Enter` | Next scene |
| `t` | Next colour theme |
| `s` | Next shader |
| `b` | Next background |
| `m` | Next mode |
| `l` | Next look |
| `p` | Next post-processing |
| `w` | Next motion while holding |
| `i` | Show or hide the caption |
| `q`, `Esc`, `Ctrl-C` | Quit |

| Option | Meaning |
| --- | --- |
| `--hold SECONDS` | How long each scene stays (default 7) |
| `--fps N` | Frames per second (default 60) |
| `--glyphs blocks\|sextants\|full` | Block characters your terminal draws well. `full` (the default) adds octants and diagonal wedges, which foot, kitty, ghostty and wezterm draw themselves; use `sextants` or `blocks` elsewhere. |
| `--watch` | Reload the shaders in `src/shaders` whenever they change; a broken edit keeps the last good shader and shows the error in the caption. |
| `--profile` | Show the GPU time of each phase in the caption (timestamp queries). |
| `--dither` | Dither colours over time for smoother gradients; every cell then changes every frame, so the terminal works about three times as hard. |

## What it shows

- **Subjects:** the ratatui mascot, the pixel-art rat from `assets/logo.svg`,
  the `RatatuiLogo` block lettering, the classic 8x8 font, the word set in a
  random font installed on the system, and logo and wordmark together.
- **Modes:** flat; a solid that turns in 3D above a reflective floor, with
  soft shadows, ambient occlusion and depth of field; dye in a swirling fluid;
  coral, mitosis, worms, mazes or spots grown by reaction-diffusion; the Game
  of Life; burning letters with rising smoke; under rippling water; or a swarm
  of particles that hold the shape.
- **Styles:** solid, outlined, neon tubes, bevelled and lit, with a drop
  shadow, extruded 3D lettering, or reflected.
- **Backgrounds:** a vignette, twinkling stars, a synthwave grid, aurora
  curtains, sun rays, Julia sets, an endless deep Mandelbrot zoom, the Burning
  Ship, Newton's fractal, a kaleidoscope, a tunnel, warp-speed stars,
  ray-marched Mandelbulb and Menger sponge fractals, fractal flames, and the
  Clifford, de Jong and Lorenz attractors.
- **Themes:** Classic, Aurora, Synthwave, Ember, Ocean, Candy, Phosphor, Prism,
  Sunset, Glacier and Toxic.
- **Shaders:** flow, plasma, pulse, sheen, rainbow, scanline, chrome, holo,
  fire, stripes, marble, lava, gold and glitter.
- **Post-processing:** bloom at three sizes, motion trails, a zoom echo, chromatic aberration,
  a curved CRT with phosphor glow, or film grain.
- **Motion while holding:** still, floating, waving, breathing, swaying, or
  letters bouncing one after another.
- **Looks:** smooth block glyphs, chunky pixels, braille dots, ASCII by
  density, ASCII by shape, an LED sign, or falling code lit by the picture.

## Transitions

Most scene changes are drawn at sub-cell resolution across the whole screen:
radial, diamond, clock, spiral, diagonal, blinds and checkerboard wipes, burn,
zoom, swirl, ripple, mosaic, slices, glitch, a CRT switching off and on, a melt
where one shape's distance field flows into the other's, and particle effects
where the old art's pixels morph, swarm, scatter, pour, spiral through a vortex
or crumble into the new art. When the look changes, a
[tachyonfx](https://github.com/ratatui/tachyonfx) transition plays instead:
dissolves, fades, sweeps, slides, evolving symbols, and terminaltexteffects-style
decrypt, matrix rain and light beams.

## How it renders

The CPU lays each scene out (fonts and bitmaps) and keeps time; everything else
runs on the GPU, through [Bevy](https://bevyengine.org)'s renderer without a
window. Each frame is a chain of compute passes (`src/shaders`):

1. **Particles**: transition particles and the physics swarm are splatted.
2. **Flames and attractors**: 65,536 points each take 24 steps, adding where
   they land to a histogram that fades a little each frame.
3. **Distance fields**: jump flooding finds, for every point, the nearest edge
   of each scene's art, kept as a grid of signed distances. They give the
   glow, outlines, neon, bevels, shadows, the 3D solid and the melt, and let
   the scene skip everything far from the art.
4. **Backgrounds** at a quarter of the samples, sampled smoothly: fractals and
   ray-marching cost a quarter as much. The Mandelbrot zoom follows a reference
   orbit computed in double precision on the CPU, so it can go to 1e-8 in f32.
5. **Simulations**: the fluid (advection, vorticity confinement, a pressure
   solve), reaction-diffusion, Life, fire and ripples; their state stays on the
   GPU between frames.
6. **Scene**: every sample (4x8 per cell) gets its background, art, light and
   particles.
7. **Bloom** at three sizes, then **post-processing** with depth of field and
   the previous frame for trails and echoes.
8. **Encode**: each cell's 32 samples are split into two colour groups; the
   split's shape picks the block glyph that matches it best (halves, quadrants,
   eighths, sextants, octants or diagonal wedges), so edges stay crisp at any
   size. The terminal's pixel size, where it reports one, keeps proportions.

One glyph and two colours per cell come back for ratatui to print. A GPU with
Vulkan, Metal or DirectX 12 drivers is needed. Most of the CPU time goes to
printing the frame; at 160x48 that is about an eighth of one core.

## Credits

The mascot and the `RatatuiLogo` lettering come from `ratatui-widgets`, and the
rat from ratatui's `assets/logo.svg`, both MIT licensed by the ratatui
developers. The octant and wedge shapes in `src/glyph_table.rs` were generated
from foot's `box-drawing.c` (MIT) by `tools/gen_glyphs.py`.
