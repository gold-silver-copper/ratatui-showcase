//! The WGSL program: its modules joined in order, checked with naga (the
//! compiler wgpu uses), and, with `--watch`, re-read from disk when they
//! change so effects can be tuned live.

use std::path::PathBuf;
use std::time::SystemTime;

/// Every module, in the order they are joined: each may use what the ones
/// before it define.
const MODULES: &[(&str, &str)] = &[
    ("common", include_str!("shaders/common.wgsl")),
    ("theme", include_str!("shaders/theme.wgsl")),
    ("art", include_str!("shaders/art.wgsl")),
    ("jfa", include_str!("shaders/jfa.wgsl")),
    ("backdrop", include_str!("shaders/backdrop.wgsl")),
    ("three_d", include_str!("shaders/three_d.wgsl")),
    ("transitions", include_str!("shaders/transitions.wgsl")),
    ("sims", include_str!("shaders/sims.wgsl")),
    ("particles", include_str!("shaders/particles.wgsl")),
    ("scene", include_str!("shaders/scene.wgsl")),
    ("post", include_str!("shaders/post.wgsl")),
    ("encode", include_str!("shaders/encode.wgsl")),
];

/// The program as built into the binary.
pub fn built_in() -> String {
    MODULES
        .iter()
        .map(|(_, source)| *source)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Checks that `source` compiles; the error names the line it is on.
pub fn validate(source: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(source).map_err(|e| e.emit_to_string(source))?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map(|_| ())
    .map_err(|e| e.emit_to_string(source))
}

/// The modules' files in the source tree, for `--watch`.
#[derive(bevy::prelude::Resource)]
pub struct Watch {
    dir: PathBuf,
    seen: Option<SystemTime>,
}

impl Watch {
    pub fn new() -> Self {
        Self {
            dir: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("shaders"),
            seen: None,
        }
    }

    /// The program from disk, if any module changed since last asked.
    pub fn changed(&mut self) -> Option<Result<String, String>> {
        let paths = MODULES
            .iter()
            .map(|(name, _)| self.dir.join(format!("{name}.wgsl")));
        let newest = paths
            .clone()
            .filter_map(|p| p.metadata().and_then(|m| m.modified()).ok())
            .max();
        if newest.is_none() || newest == self.seen {
            return None;
        }
        let first = self.seen.is_none();
        self.seen = newest;
        if first {
            return None;
        }
        let read: Result<Vec<String>, String> = paths
            .map(|p| std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display())))
            .collect();
        Some(read.map(|sources| sources.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn program_is_valid_wgsl() {
        let result = super::validate(&super::built_in());
        assert!(result.is_ok(), "{}", result.err().unwrap_or_default());
    }
}
