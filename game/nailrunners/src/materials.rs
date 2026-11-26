//! The game's material layers: texture files, shading, and where assets live on disk.

use std::path::{Path, PathBuf};

use rivet_assets::{AssetError, Rgba8Image, load_rgba, mip_chain, resized};
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
    StoneWall = 7,
    Cobblestone = 8,
    Flagstone = 9,
    TechPanel = 10,
    CityFacade = 11,
    WoodPlanks = 12,
}

impl Layer {
    /// Layers loaded from `LAYER_FILES`.
    pub const COUNT: usize = 13;
}

/// The kart's own texture, appended after the file layers.
pub const KART_LAYER: u32 = Layer::COUNT as u32;

pub const LAYER_FILES: [&str; Layer::COUNT] = [
    "asphalt.png",
    "dirt.png",
    "sand.png",
    "rock.png",
    "concrete.png",
    "metal_rust.png",
    "grass.png",
    "stone_wall.png",
    "cobblestone.png",
    "flagstone.png",
    "tech_panel.png",
    "city_facade.png",
    "wood_planks.png",
];

/// Specular, gloss and triplanar scale per layer.
pub fn shading() -> [MaterialShading; Layer::COUNT + 1] {
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
        m(0.05, 14.0, 0.25), // castle stone wall
        m(0.10, 20.0, 0.25), // cobblestone
        m(0.04, 12.0, 0.20), // flagstone
        m(0.45, 60.0, 0.20), // tech deck
        m(0.50, 48.0, 0.03), // skyscraper facades
        m(0.06, 12.0, 0.30), // planks
        m(0.30, 32.0, 0.25), // kart paint and metal
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
    let images = LAYER_FILES
        .iter()
        .map(|f| load_rgba(&assets.join("textures").join(f)))
        .collect::<Result<Vec<_>, _>>()?;
    // The texture array needs every layer at one size; match the first layer.
    let (w, h) = (images[0].width, images[0].height);
    Ok(images
        .iter()
        .map(|img| {
            if (img.width, img.height) != (w, h) {
                log::warn!(
                    "texture layer is {}x{}, resizing to {w}x{h}",
                    img.width,
                    img.height
                );
            }
            mip_chain(&resized(img, w, h))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_tables_line_up() {
        assert_eq!(LAYER_FILES.len(), Layer::COUNT);
        assert_eq!(shading().len(), Layer::COUNT + 1);
        assert_eq!(LAYER_FILES[Layer::Rock as usize], "rock.png");
    }

    #[test]
    fn mismatched_layer_sizes_are_resized_to_the_first() {
        let root = std::env::temp_dir().join(format!("rivet-layers-test-{}", std::process::id()));
        let tex = root.join("textures");
        std::fs::create_dir_all(&tex).unwrap();
        for (i, f) in LAYER_FILES.iter().enumerate() {
            let size = if i == 2 { 16 } else { 32 };
            image::RgbaImage::from_pixel(size, size, image::Rgba([90, 80, 70, 255]))
                .save(tex.join(f))
                .unwrap();
        }
        let layers = load_layers(&root).unwrap();
        assert!(
            layers
                .iter()
                .all(|l| (l[0].width, l[0].height) == (32, 32) && l.len() == layers[0].len())
        );
        std::fs::remove_dir_all(&root).unwrap();
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
