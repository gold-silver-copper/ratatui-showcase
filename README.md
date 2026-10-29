# ratatui-showcase

A fullscreen [ratatui](https://ratatui.rs) demo screen for big monitors and
projectors. The ratatui logo and the "ratatui" wordmark take turns, scaled to
fill any terminal while keeping their proportions, with
[tachyonfx](https://github.com/ratatui/tachyonfx) transitions in the style of
[Omarchy](https://omarchy.org)'s screensaver.

```sh
cargo run --release
cargo run --release -- --hold 10 --glyphs sextants
cargo run --release -- --seed 1234 # the scenes of seed 1234 again
cargo run --release -- --watch     # edit src/shaders and watch it change
cargo run --release -- --profile   # GPU and terminal cost in the caption
cargo run --release -- --pin post=paint --pin mode=slime   # keep layers fixed
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
| `--hold SECONDS` | How long each scene stays, 0.5 to 86400 (default 7), rounded to whole bars of the tempo: 7 is 4 bars at 120 bpm |
| `--bpm N` | The show's tempo, 40 to 240 (default 120) |
| `--seed N` | Which scenes play. The caption shows the seed; the same seed on a terminal of the same size plays the same scenes again. Without it, a random one. |
| `--fps N` | Frames per second, 1 to 1000 (default 60). Simulations run at the same speed at any rate. |
| `--glyphs blocks\|sextants\|full` | Block characters your terminal draws well. `full` (the default) adds octants and diagonal wedges, which foot, kitty, ghostty and wezterm draw themselves; use `sextants` or `blocks` elsewhere. |
| `--watch` | Reload the shaders in `src/shaders` whenever they change; an edit that doesn't compile, or whose pipelines fail to build, leaves the running shader in place and shows the error in the caption. |
| `--profile` | Show the GPU time of each phase in the caption (timestamp queries). |
| `--pin LAYER=CHOICE` | Keep a layer in every scene, whatever the director would pick: `theme`, `shader`, `backdrop`, `look`, `style`, `mode`, `post` or `motion`, such as `post=paint`, `shader=plasma`, `backdrop=mandelbrot` or `theme=ember`. Repeatable. |
| `--dither` | Dither colours with blue noise moving on by the golden ratio each frame, for smoother gradients; cells then change every frame, so the terminal works harder. |

## What it shows

- **Subjects:** the ratatui mascot, the pixel-art rat from `assets/logo.svg`,
  the `RatatuiLogo` block lettering, the classic 8x8 font, the word set in a
  random font installed on the system (picked by family, so every design gets
  a turn), a ransom note with each letter from a different font, or logo and
  wordmark together.
- **Modes:** flat; a solid that turns in 3D above a reflective floor, with
  soft shadows, ambient occlusion and depth of field; dye in a swirling fluid;
  coral, mitosis, worms, mazes or spots grown by reaction-diffusion; the Game
  of Life; burning letters with rising smoke; a swarm
  of particles that hold the shape; slime mold whose network of veins grows inside the
  letters; a flock of boids that gathers into them; letters crumbling into
  falling sand; crystals branching until they fill the letters; ink rising
  from them and curling through water; or Lenia, continuous life whose
  creatures hatch from the letters.
- **Styles:** solid, outlined, neon tubes, bevelled and lit, with a drop
  shadow, extruded 3D lettering, or reflected.
- **Backgrounds:** a vignette, twinkling stars, a synthwave grid, aurora
  curtains, sun rays, Julia sets walking the Mandelbrot set's boundary, an
  deep Mandelbrot zoom that dives to a copy of the art, the Burning
  Ship, Newton's fractal, a kaleidoscope, a tunnel, warp-speed stars,
  ray-marched Mandelbulb and Menger sponge fractals, a flight over ray-marched
  hills, metaballs, stained glass, marbled paper, the Apollonian gasket, a
  Kleinian group's limit set by Jos Leys' escape-time algorithm, hyperbolic
  tilings of the Poincare disk gliding under Mobius maps, domain colouring of
  a complex function, a ray-marched Mandelbox and kaleidoscopic IFS, the
  scene's art as a collage of copies of itself (by Barnsley's collage
  theorem) slowly zooming in, fractal flames, the Clifford, de Jong and Lorenz
  attractors, and Particle Life. The Julia sets and the Mandelbrot zoom use
  the scene's art as an orbit trap: where an escaping orbit passes through
  the logo or the wordmark, they light up, so the fractal draws bent copies
  of it.
- **Colours:** the themes Classic, Aurora, Synthwave, Ember, Ocean, Candy,
  Phosphor, Prism, Sunset, Glacier and Toxic, or a palette made up for the
  scene: an analogous, complementary, triadic, split, tetradic or tonal
  harmony around a random hue, in OKLCH so its colours look evenly bright.
- **Shaders:** flow, plasma, pulse, sheen, rainbow, scanline, chrome, holo,
  fire, stripes, marble, lava, gold and glitter.
- **Post-processing:** bloom at three sizes, motion trails, a zoom echo, chromatic aberration,
  a curved CRT with phosphor glow, film grain, shafts of light streaming
  out from the art, oil paint (brush strokes along the picture's edges, lit as ridges of paint, over an anisotropic Kuwahara underpainting), silk (line
  integral convolution along the fluid or the picture's edges), or the
  Droste effect spiralling the picture into itself.
- **Motion while holding:** still, floating, waving, breathing, swaying,
  letters bouncing one after another, letters dropping into place, or a
  ticker scrolling across.
- **Looks:** smooth block glyphs, chunky pixels, braille dots, ASCII by
  density, ASCII by shape, an LED sign, falling code lit by the picture,
  contour lines in box-drawing characters, halftone dots, or the picture
  spelled out in this README.

## Between scenes

After each scene a card holds for at least 10 seconds, drawn with ratatui's
own widgets in the colours of the scene before it, calm enough to read from
the back of a room: "ratatui" in the classic 8x8 font, one fact about it
typed out, and a dashboard of built-in widgets with live data, each box
titled with the widget's name: a world map that draws itself on a `Canvas`
before routes fly between cities, a scrolling `Chart` and `Sparkline`, a
`BarChart`, a `Table` of apps built with ratatui, a `LineGauge`, and a
`Gauge` counting down to the next scene.

## How scenes are chosen

Every choice of every layer has a busyness, from plain (a vignette, the
smooth look, flat art) to enough to lead the scene (a deep fractal, a
simulation, falling code). A scene's busyness, plus a penalty for pairs
known to clash (character looks under film grain or oil paint, thin neon
strokes in a character look, a busy shader over a busy fractal), stays under
a budget, and at most one layer leads. That still leaves about 2.9 million
combinations of shader, background, look, style, mode, post-processing and
motion, before palettes and subjects (26 million without the budget).

Scenes come in short runs of three to five, each from a pattern: building up
to a peak and calming down, a slow burn, one background under three looks
or in several moods, a tour of backgrounds, the art itself leading, or a
peak and its echoes. A run starts from scratch; each cut after that changes
one or two layers, and the hand-tuned presets are the peaks. Among the
choices that fit, the ones not seen for longest are the likeliest, so over
hours every choice keeps coming up. The look only changes with a cell
transition; every other cut is drawn at sub-cell resolution.

One tempo runs the show: cuts land on bar lines, the accents on a later
one, transitions last whole beats, and the motions while holding float,
wave, breathe and hop in time.

The art stays readable whatever plays behind it. Around it, the background
is dimmed just as far as the art's colours need to keep a contrast of 3:1
(WCAG's for large text), measured per sample on the GPU from the distance
field and checked again after bloom and the other effects: a dark
background is left alone, a bright fractal gets a dark halo. A logo and
wordmark together that would leave either under a sixth of the screen's
height show the logo alone.

Every choice comes from one seeded generator, shown in the caption.

## Transitions

Most scene changes are drawn at sub-cell resolution across the whole screen:
radial, diamond, clock, spiral, diagonal, blinds and checkerboard wipes, burn,
zoom, swirl, ripple, mosaic, slices, glitch, a CRT switching off and on, a melt
where one shape's distance field flows into the other's, and particle effects
where the old art's pixels morph, swarm, scatter, pour, spiral through a vortex
or crumble into the new art; the old scene shattering like glass, peeling off
like a page, smearing into sorted rows, or the new one soaking in like ink;
and two run by the simulations: the old scene burning away in the fire, or
dissolving into the fluid as dye. When the look changes, a
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
   glow, outlines, neon, bevels, shadows, the 3D solid, the melt, the orbit
   traps and the halo that keeps the art readable, and let the scene skip
   everything far from the art. While the art holds still they are kept from
   frame to frame.

4. **Backgrounds** at a quarter of the samples, sampled smoothly: fractals and
   ray-marching cost a quarter as much. The Mandelbrot zoom follows a reference
   orbit computed in double precision on the CPU, so it can go to 1e-8 in f32.
   Two zooms in three are aimed: while the scene before holds, the CPU
   searches (within 400 ms and 400 tries) for a spot where the orbit trap
   shaped like the art draws a clear, upright copy of it at the end of the
   dive, by solving with Newton's method for a point whose orbit lands on
   the art's centre at the right step, and checks it with a small render
   done the way the GPU does it (`src/zoom.rs`). If nothing turns up, the
   scene keeps the endless zoom into seahorse valley. The set is lit by its
   distance estimate, so its filaments glow at any depth.
5. **Simulations**: the fluid (advection, vorticity confinement, a pressure
   solve), reaction-diffusion, Life and fire, stepped 60 times a second
   whatever the frame rate; their state stays on the GPU between frames. When
   the next scene simulates too, the old one's last look is frozen for the
   transition; otherwise it keeps running until it is gone.
6. **Scene**: every sample (4x8 per cell) gets its background, art, light and
   particles.
7. **Bloom** at three sizes, then **post-processing** with depth of field and
   the previous frame for trails and echoes.
8. **Encode**: each cell's 32 samples are split into two colour groups; the
   split's shape picks the block glyph that matches it best (halves, quadrants,
   eighths, sextants, octants or diagonal wedges): the closest few by shape,
   then the one whose blurred shape is closest to the blurred picture, so a
   stroke off by a sample still finds its glyph. Edges stay crisp at any
   size. The terminal's pixel size, where it reports one, keeps proportions.

One glyph and two colours per cell come back for ratatui to print; a cell that
changed too little to see keeps what it had, so the terminal isn't sent it
again, and each frame goes out as one synchronized update. A GPU with Vulkan,
Metal or DirectX 12 drivers is needed; a terminal too large for its buffers
gets a message instead of a frame. Most of the CPU time goes to printing the
frame; at 160x48 that is about a fifth of one core.

Where two glyphs fit a cell almost equally well, the cell keeps the one it
shows unless the new one fits clearly better (hysteresis), so noise doesn't
make it flicker. The changed cells go out with the shortest escape codes
(a space's foreground and a full block's background are never sent) in one
write. A terminal that can't keep up makes that write block: the screen
then lets colours drift further before resending a cell and draws at a
steady lower rate, and comes back once writes are quick again. With
`--profile` the caption shows frames per second, bytes, cells changed and
changed back per frame, and the write time.

The layout of the parameters the shaders read, and every option's number,
come from the Rust side (`src/params.rs`, `src/shader.rs`), so the two can't
drift apart.

## Credits

The mascot and the `RatatuiLogo` lettering come from `ratatui-widgets`, and the
rat from ratatui's `assets/logo.svg`, both MIT licensed by the ratatui
developers. The octant and wedge shapes in `src/glyph_table.rs` were generated
from foot's `box-drawing.c` (MIT) by `tools/gen_glyphs.py`.
