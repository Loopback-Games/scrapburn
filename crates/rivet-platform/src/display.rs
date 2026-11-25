//! Which GPUs have a monitor attached (Linux sysfs). Used to pick a GPU that can present.

use std::path::Path;

fn read_hex(path: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(path).ok()?;
    u32::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

/// PCI `(vendor, device)` ids of GPUs with at least one connected display, read from a DRM
/// sysfs directory laid out like `/sys/class/drm`.
pub fn connected_display_gpus(drm_root: &Path) -> Vec<(u32, u32)> {
    let Ok(entries) = std::fs::read_dir(drm_root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        // Connector entries look like "card1-DP-2".
        let Some((card, _)) = name.split_once('-') else {
            continue;
        };
        if !card.starts_with("card") {
            continue;
        }
        let connected = std::fs::read_to_string(drm_root.join(&name).join("status"))
            .is_ok_and(|s| s.trim() == "connected");
        if !connected {
            continue;
        }
        let dev = drm_root.join(card).join("device");
        if let (Some(v), Some(d)) = (read_hex(&dev.join("vendor")), read_hex(&dev.join("device")))
            && !out.contains(&(v, d))
        {
            out.push((v, d));
        }
    }
    out
}

/// GPUs driving a display on this machine; empty when unknown (non-Linux, no sysfs).
pub fn display_gpus() -> Vec<(u32, u32)> {
    if cfg!(target_os = "linux") {
        connected_display_gpus(Path::new("/sys/class/drm"))
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_connected_card() {
        let root = std::env::temp_dir().join(format!("rivet-drm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (card, vendor, device) in [("card0", "0x8086", "0x4682"), ("card1", "0x10de", "0x2487")]
        {
            fs::create_dir_all(root.join(card).join("device")).unwrap();
            fs::write(root.join(card).join("device/vendor"), format!("{vendor}\n")).unwrap();
            fs::write(root.join(card).join("device/device"), format!("{device}\n")).unwrap();
        }
        for (conn, status) in [
            ("card0-DP-1", "disconnected"),
            ("card1-DP-2", "connected"),
            ("card1-DP-3", "disconnected"),
        ] {
            fs::create_dir_all(root.join(conn)).unwrap();
            fs::write(root.join(conn).join("status"), format!("{status}\n")).unwrap();
        }
        assert_eq!(connected_display_gpus(&root), vec![(0x10de, 0x2487)]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_root_is_empty() {
        assert!(connected_display_gpus(std::path::Path::new("/nonexistent/rivet")).is_empty());
    }
}
