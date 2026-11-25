//! Picking a GPU adapter. Min-spec-first: `Auto` prefers the integrated GPU.

use std::fmt;
use std::str::FromStr;

/// Which GPU to run on. Parsed from `--adapter` or the `RIVET_ADAPTER` env var.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AdapterPreference {
    /// Integrated first (the min-spec target), then discrete, then anything.
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
}

impl AdapterDesc {
    pub fn from_info(info: &wgpu::AdapterInfo) -> Self {
        Self {
            name: info.name.clone(),
            device_type: info.device_type,
            backend: info.backend,
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
            },
            AdapterDesc {
                name: "Intel(R) UHD Graphics 730 (ADL-S GT1)".into(),
                device_type: DeviceType::IntegratedGpu,
                backend: Backend::Vulkan,
            },
            AdapterDesc {
                name: "llvmpipe (LLVM 20)".into(),
                device_type: DeviceType::Cpu,
                backend: Backend::Vulkan,
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
    fn discrete_falls_back_error() {
        let only_igpu = vec![list().remove(1)];
        assert!(matches!(
            select_adapter(&only_igpu, &AdapterPreference::Discrete),
            Err(AdapterError::NoMatch { .. })
        ));
    }
}
