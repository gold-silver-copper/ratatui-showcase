//! The layout of the `params` buffer the shaders read. `shader.rs` turns
//! these lists into WGSL constants, so neither side counts words by hand.

/// Declares slots: an enum, and each slot's name and length in words.
macro_rules! slots {
    ($(#[$meta:meta])* $name:ident { $($(#[$doc:meta])* $slot:ident $(: $len:literal)?),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($(#[$doc])* $slot),* }

        impl $name {
            /// Every slot, its name and its length in words, in order.
            pub const ALL: &[(Self, &'static str, usize)] =
                &[$((Self::$slot, stringify!($slot), slots!(@len $($len)?))),*];

            /// The slot's first word.
            pub fn at(self) -> usize {
                Self::ALL
                    .iter()
                    .take_while(|(slot, _, _)| *slot != self)
                    .map(|(_, _, len)| len)
                    .sum()
            }
        }
    };
    (@len) => { 1 };
    (@len $len:literal) => { $len };
}

slots! {
    /// The frame's header: what applies to the whole frame.
    H {
        Cols,
        Rows,
        Time,
        /// Beats of the show's tempo, wrapped every 64.
        Beat,
        /// The screen's width over its height.
        Aspect,
        Look,
        Glyphs,
        /// The transition, 0 for a scene at rest.
        Kind,
        Progress,
        /// Transition particles' size in samples.
        Grain,
        Sources,
        Targets,
        Frame,
        Dt,
        Post,
        Dither,
        /// Swarm particles, and the block of the scene they draw.
        Swarm,
        SwarmScene,
        /// The block of the scene whose simulation runs, its id, and how
        /// many steps it takes this frame.
        SimScene,
        SimId,
        SimSteps,
        /// The scene the distance fields can be kept for, or 0.
        FieldKey,
        /// Which splatted backdrop the flame points draw, or 0.
        SplatId,
        /// How much of the flame histogram survives this frame.
        Fade,
        /// Where each part of `inputs` starts, and how long some are.
        InGlyphs,
        InAscii,
        AsciiCount,
        InPoints,
        InHomes,
        Homes,
        InArt,
        InOrbit,
        OrbitLen,
        FlameThreads,
        FlameSteps,
        /// The README's characters, for the text look.
        InText,
        TextLen,
        /// The blue-noise mask, and Lenia's kernel.
        InNoise,
        InKernel,
        /// Where each region of `state` starts (`Region`).
        Regions: 19,
    }
}

slots! {
    /// Regions of the `state` buffer, as named in the header.
    Region {
        Scene,
        Bloom0,
        Bloom1,
        Bloom2,
        Bloom3,
        Bloom4,
        Bloom5,
        Field0,
        Field1,
        Sim0,
        Sim1,
        Sim2,
        Sim3,
        /// The old scene's simulation, frozen when a new one takes over.
        Snapshot,
        Particles,
        /// Slime mold agents and crystal walkers.
        Agents,
        BgOld,
        BgNew,
        Flame,
    }
}

slots! {
    /// A scene's block: the old scene's follows the header, the new one's
    /// follows that.
    S {
        Shader,
        Backdrop,
        Idle,
        Amplitude,
        Pieces,
        Stops,
        Mode,
        Style,
        Background: 3,
        Accent: 3,
        Stop0: 15,
        Age,
        Width,
        Glow,
        Variant,
        Piece0: 12,
    }
}

/// Words in the header and in each scene's block.
pub const HEADER: usize = 64;
pub const BLOCK: usize = 64;
/// Where the old and the new scene's blocks start.
pub const FROM: usize = HEADER;
pub const TO: usize = HEADER + BLOCK;
/// Words per piece of art: x, y, width, height, glow and where its texels start.
pub const PIECE_LEN: usize = 6;

/// Header words, written by slot.
pub struct Words(Vec<u32>);

impl Words {
    pub fn new(len: usize) -> Self {
        Self(vec![0; len])
    }

    pub fn put(&mut self, at: usize, value: u32) {
        if let Some(slot) = self.0.get_mut(at) {
            *slot = value;
        }
    }

    pub fn into_vec(self) -> Vec<u32> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_fit_their_blocks() {
        fn end<T>(all: &[(T, &str, usize)]) -> usize {
            all.iter().map(|(_, _, len)| len).sum()
        }
        assert!(end(H::ALL) <= HEADER);
        assert!(end(S::ALL) <= BLOCK);
        // Room for two pieces.
        assert!(
            S::ALL
                .iter()
                .any(|&(s, _, len)| s == S::Piece0 && len == 2 * PIECE_LEN)
        );
        assert_eq!(H::Regions.at() + Region::ALL.len(), end(H::ALL));
    }
}
