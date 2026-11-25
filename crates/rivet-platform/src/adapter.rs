//! Picking a GPU adapter. `Auto` prefers the GPU driving a display, then integrated.

use std::fmt;
use std::str::FromStr;

/// Which GPU to run on. Parsed from `--adapter` or the `RIVET_ADAPTER` env var.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AdapterPreference {
    /// The GPU driving a display first, then integrated, discrete, anything (`candidate_order`).
    #[default]
    Auto,
    Integrated,
    Discrete,
    /// Case-insensitive substring of the adapter name.
    NameContains(String),
}

impl FromStr for AdapterPreference {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "" | "auto" => Self::Auto,
            "integrated" | "igpu" => Self::Integrated,
            "discrete" | "dgpu" => Self::Discrete,
            other => Self::NameContains(other.to_string()),
        })
    }
}

impl fmt::Display for AdapterPreference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Auto => f.write_str("auto"),
            Self::Integrated => f.write_str("integrated"),
            Self::Discrete => f.write_str("discrete"),
            Self::NameContains(s) => write!(f, "name containing \"{s}\""),
        }
    }
}

/// What selection needs to know about one adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterDesc {
    pub name: String,
    pub device_type: wgpu::DeviceType,
    pub backend: wgpu::Backend,
    /// PCI vendor and device ids (0 when the backend doesn't report them).
    pub vendor: u32,
    pub device: u32,
}

impl AdapterDesc {
    pub fn from_info(info: &wgpu::AdapterInfo) -> Self {
        Self {
            name: info.name.clone(),
            device_type: info.device_type,
            backend: info.backend,
            vendor: info.vendor,
            device: info.device,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdapterError {
    #[error("no GPU adapter matches {wanted}; available adapters:\n{}", list_adapters(.available))]
    NoMatch {
        wanted: String,
        available: Vec<AdapterDesc>,
    },
    #[error(
        "{name} matches {wanted} but cannot present to this window (no monitor on that GPU?); \
         available adapters:\n{}",
        list_adapters(.available)
    )]
    CannotPresent {
        name: String,
        wanted: String,
        available: Vec<AdapterDesc>,
    },
}

fn list_adapters(list: &[AdapterDesc]) -> String {
    if list.is_empty() {
        return "  (none)".into();
    }
    list.iter()
        .map(|a| format!("  {} ({:?}, {:?})", a.name, a.device_type, a.backend))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Adapters to try, best first, so a caller can fall through when one can't present.
/// `Auto` puts GPUs that drive a display (`display_gpus`, PCI vendor/device ids) first — the
/// system's own choice on desktops and hybrid laptops — then integrated, discrete, the rest.
/// Explicit requests list only their match.
pub fn candidate_order(
    list: &[AdapterDesc],
    pref: &AdapterPreference,
    display_gpus: &[(u32, u32)],
) -> Vec<usize> {
    use wgpu::DeviceType as T;
    match pref {
        AdapterPreference::Auto => {
            let rank = |a: &AdapterDesc| {
                let drives_display = display_gpus.contains(&(a.vendor, a.device));
                let by_type = match a.device_type {
                    T::IntegratedGpu => 0,
                    T::DiscreteGpu => 1,
                    _ => 2,
                };
                (!drives_display, by_type)
            };
            let mut idx: Vec<usize> = (0..list.len()).collect();
            idx.sort_by_key(|&i| rank(&list[i]));
            idx
        }
        _ => select_adapter(list, pref)
            .map(|i| vec![i])
            .unwrap_or_default(),
    }
}

/// Adapters (indices into `all`) to try for a window, best first, keeping only those whose
/// `presentable[i]` is true. Errors list every adapter, including ones that can't present.
pub fn window_candidates(
    all: &[AdapterDesc],
    presentable: &[bool],
    pref: &AdapterPreference,
    display_gpus: &[(u32, u32)],
) -> Result<Vec<usize>, AdapterError> {
    let order = candidate_order(all, pref, display_gpus);
    let usable: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| presentable.get(i) == Some(&true))
        .collect();
    if !usable.is_empty() {
        return Ok(usable);
    }
    Err(match order.first() {
        Some(&i) => AdapterError::CannotPresent {
            name: all[i].name.clone(),
            wanted: pref.to_string(),
            available: all.to_vec(),
        },
        None => AdapterError::NoMatch {
            wanted: pref.to_string(),
            available: all.to_vec(),
        },
    })
}

/// Returns the index of the adapter to use. Explicit requests never silently fall back.
pub fn select_adapter(
    list: &[AdapterDesc],
    pref: &AdapterPreference,
) -> Result<usize, AdapterError> {
    use wgpu::DeviceType as T;
    let by_type = |t: T| list.iter().position(|a| a.device_type == t);
    let found = match pref {
        AdapterPreference::Auto => by_type(T::IntegratedGpu)
            .or_else(|| by_type(T::DiscreteGpu))
            .or_else(|| (!list.is_empty()).then_some(0)),
        AdapterPreference::Integrated => by_type(T::IntegratedGpu),
        AdapterPreference::Discrete => by_type(T::DiscreteGpu),
        AdapterPreference::NameContains(needle) => {
            let needle = needle.to_ascii_lowercase();
            list.iter()
                .position(|a| a.name.to_ascii_lowercase().contains(&needle))
        }
    };
    found.ok_or_else(|| AdapterError::NoMatch {
        wanted: pref.to_string(),
        available: list.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::{Backend, DeviceType};

    fn list() -> Vec<AdapterDesc> {
        vec![
            AdapterDesc {
                name: "NVIDIA GeForce RTX 3060".into(),
                device_type: DeviceType::DiscreteGpu,
                backend: Backend::Vulkan,
                vendor: 0,
                device: 0,
            },
            AdapterDesc {
                name: "Intel(R) UHD Graphics 730 (ADL-S GT1)".into(),
                device_type: DeviceType::IntegratedGpu,
                backend: Backend::Vulkan,
                vendor: 0,
                device: 0,
            },
            AdapterDesc {
                name: "llvmpipe (LLVM 20)".into(),
                device_type: DeviceType::Cpu,
                backend: Backend::Vulkan,
                vendor: 0,
                device: 0,
            },
        ]
    }

    #[test]
    fn auto_prefers_integrated() {
        assert_eq!(
            select_adapter(&list(), &AdapterPreference::Auto).unwrap(),
            1
        );
    }

    #[test]
    fn name_contains_case_insensitive() {
        let pref: AdapterPreference = "uhd".parse().unwrap();
        assert_eq!(select_adapter(&list(), &pref).unwrap(), 1);
        let pref: AdapterPreference = "Discrete".parse().unwrap();
        assert_eq!(pref, AdapterPreference::Discrete);
    }

    #[test]
    fn no_match_lists_all() {
        let err =
            select_adapter(&list(), &AdapterPreference::NameContains("radeon".into())).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("RTX 3060"), "{msg}");
        assert!(msg.contains("UHD Graphics 730"), "{msg}");
    }

    #[test]
    fn candidate_order_auto_and_explicit() {
        assert_eq!(
            candidate_order(&list(), &AdapterPreference::Auto, &[]),
            vec![1, 0, 2]
        );
        assert_eq!(
            candidate_order(&list(), &AdapterPreference::Discrete, &[]),
            vec![0]
        );
        assert!(
            candidate_order(
                &list(),
                &AdapterPreference::NameContains("radeon".into()),
                &[]
            )
            .is_empty()
        );
    }

    #[test]
    fn candidate_order_prefers_display_gpu() {
        let mut l = list();
        l[0].vendor = 0x10de;
        l[0].device = 0x2487;
        assert_eq!(
            candidate_order(&l, &AdapterPreference::Auto, &[(0x10de, 0x2487)]),
            vec![0, 1, 2]
        );
        // Explicit requests ignore display hints.
        assert_eq!(
            candidate_order(&l, &AdapterPreference::Integrated, &[(0x10de, 0x2487)]),
            vec![1]
        );
    }

    #[test]
    fn window_candidates_report_all_adapters() {
        // Only the iGPU can present to this window.
        let presentable = [false, true, false];
        let err = window_candidates(&list(), &presentable, &AdapterPreference::Discrete, &[])
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("RTX 3060") && msg.contains("cannot present"),
            "{msg}"
        );
        assert!(
            msg.contains("UHD Graphics 730") && msg.contains("llvmpipe"),
            "{msg}"
        );
        let err = window_candidates(
            &list(),
            &presentable,
            &AdapterPreference::NameContains("radeon".into()),
            &[],
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("RTX 3060") && err.contains("llvmpipe"),
            "{err}"
        );
        assert_eq!(
            window_candidates(&list(), &presentable, &AdapterPreference::Auto, &[]).unwrap(),
            vec![1]
        );
    }

    #[test]
    fn discrete_falls_back_error() {
        let only_igpu = vec![list().remove(1)];
        assert!(matches!(
            select_adapter(&only_igpu, &AdapterPreference::Discrete),
            Err(AdapterError::NoMatch { .. })
        ));
    }
}
