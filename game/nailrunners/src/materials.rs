//! The game's material layers: texture files, shading, and where assets live on disk.

use std::path::{Path, PathBuf};

use rivet_assets::{AssetError, Rgba8Image, load_rgba, mip_chain};
use rivet_render::MaterialShading;

/// Texture array layers, in `LAYER_FILES` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Asphalt = 0,
    Dirt = 1,
    Sand = 2,
    Rock = 3,
    Concrete = 4,
    MetalRust = 5,
    Grass = 6,
}

impl Layer {
    pub const COUNT: usize = 7;
}

pub const LAYER_FILES: [&str; Layer::COUNT] = [
    "asphalt.png",
    "dirt.png",
    "sand.png",
    "rock.png",
    "concrete.png",
    "metal_rust.png",
    "grass.png",
];

/// Specular, gloss and triplanar scale per layer.
pub fn shading() -> [MaterialShading; Layer::COUNT] {
    let m = |specular, gloss, triplanar_scale| MaterialShading {
        specular,
        gloss,
        triplanar_scale,
    };
    [
        m(0.18, 28.0, 0.15), // asphalt: slight sheen
        m(0.03, 8.0, 0.12),  // dirt
        m(0.03, 10.0, 0.10), // sand
        m(0.05, 12.0, 0.07), // rock (cliffs, triplanar)
        m(0.06, 14.0, 0.20), // concrete
        m(0.35, 40.0, 0.25), // rusty metal
        m(0.02, 8.0, 0.12),  // grass
    ]
}

/// The first `assets` directory (containing `textures/`) found walking up from each start path.
pub fn find_assets_dir_from(starts: &[PathBuf]) -> Option<PathBuf> {
    for start in starts {
        let mut dir: Option<&Path> = Some(start.as_path());
        while let Some(d) = dir {
            let candidate = d.join("assets");
            if candidate.join("textures").is_dir() {
                return Some(candidate);
            }
            dir = d.parent();
        }
    }
    None
}

/// `RIVET_ASSETS`, else walk up from the executable and the working directory.
pub fn find_assets_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RIVET_ASSETS") {
        return Some(PathBuf::from(p));
    }
    let mut starts = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        starts.push(dir.to_path_buf());
    }
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    find_assets_dir_from(&starts)
}

/// Loads every layer and builds its mip chain.
pub fn load_layers(assets: &Path) -> Result<Vec<Vec<Rgba8Image>>, AssetError> {
    LAYER_FILES
        .iter()
        .map(|f| load_rgba(&assets.join("textures").join(f)).map(|img| mip_chain(&img)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_tables_line_up() {
        assert_eq!(LAYER_FILES.len(), Layer::COUNT);
        assert_eq!(shading().len(), Layer::COUNT);
        assert_eq!(LAYER_FILES[Layer::Rock as usize], "rock.png");
    }

    #[test]
    fn finds_assets_dir_by_walking_up() {
        let root = std::env::temp_dir().join(format!("rivet-assets-test-{}", std::process::id()));
        let deep = root.join("a/b/c");
        std::fs::create_dir_all(root.join("assets/textures")).unwrap();
        std::fs::create_dir_all(&deep).unwrap();
        assert_eq!(
            find_assets_dir_from(std::slice::from_ref(&deep)),
            Some(root.join("assets"))
        );
        assert_eq!(find_assets_dir_from(&[std::path::PathBuf::from("/")]), None);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
