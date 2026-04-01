//! Slide manifest parsing and validation.
//!
//! This module handles the parsing and validation of `manifest.json` files
//! from slide packages.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::transition::TransitionKind;

/// Minimum allowed display duration in seconds.
pub const MIN_DISPLAY_DURATION_SECONDS: u32 = 1;
/// Maximum allowed display duration in seconds.
pub const MAX_DISPLAY_DURATION_SECONDS: u32 = 300;

/// Slide manifest structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SlideManifest {
    /// Slide name.
    pub name: Option<String>,
    /// Slide version.
    pub version: Option<String>,
    /// Slide author.
    pub author: Option<String>,
    /// Slide description.
    pub description: Option<String>,
    /// ABI version (must match engine ABI).
    pub abi_version: Option<u32>,
    /// Scene space ("screen_2d" or "world_3d").
    pub scene_space: Option<String>,
    /// Asset references.
    pub assets: Option<ManifestAssets>,
    /// Shader overrides.
    pub shaders: Option<ManifestShaders>,
    /// Display configuration.
    pub display: Option<DisplayConfig>,
    /// Hardware requirements.
    pub requirements: Option<ManifestRequirements>,
    /// Sidecar configuration.
    pub sidecar: Option<ManifestSidecar>,
}

/// Asset references in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ManifestAssets {
    /// Texture assets.
    #[serde(default)]
    pub textures: Vec<AssetRef>,
    /// Mesh assets.
    #[serde(default)]
    pub meshes: Vec<AssetRef>,
    /// Scene assets.
    #[serde(default)]
    pub scenes: Vec<SceneAssetRef>,
}

/// Reference to a texture or mesh asset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetRef {
    /// Path to the asset file.
    pub path: String,
    /// Usage hint (e.g., "material", "font", "detail").
    #[serde(default)]
    pub usage: Option<String>,
    /// Slot index for the asset.
    #[serde(default)]
    pub slot: Option<usize>,
    /// Label for the asset.
    #[serde(default)]
    pub label: Option<String>,
    /// Unique ID for the asset.
    #[serde(default)]
    pub id: Option<String>,
}

/// Reference to a scene asset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SceneAssetRef {
    /// Path to the scene file.
    pub path: String,
    /// Label for the scene.
    #[serde(default)]
    pub label: Option<String>,
    /// Unique ID for the scene.
    #[serde(default)]
    pub id: Option<String>,
    /// Entry camera name.
    #[serde(default)]
    pub entry_camera: Option<String>,
    /// Compile profile for the scene.
    #[serde(default)]
    pub compile_profile: Option<String>,
}

/// Shader overrides in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ManifestShaders {
    /// Vertex shader path.
    pub vertex: Option<String>,
    /// Fragment shader path.
    pub fragment: Option<String>,
}

/// Display configuration in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DisplayConfig {
    /// Display duration in seconds.
    pub duration_seconds: Option<u32>,
    /// Transition-in kind.
    pub transition_in: Option<String>,
    /// Transition-out kind.
    pub transition_out: Option<String>,
}

/// Hardware requirements in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ManifestRequirements {
    /// Minimum texture dimension.
    pub min_texture_dim: Option<u32>,
    /// Whether depth buffer is used.
    pub uses_depth_buffer: Option<bool>,
    /// Whether transparency is used.
    pub uses_transparency: Option<bool>,
}

/// Sidecar configuration in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ManifestSidecar {
    /// WASI preopen directories.
    #[serde(default)]
    pub wasi_preopens: Vec<String>,
}

/// Manifest validation errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestValidationError {
    /// ABI version mismatch.
    #[error("abi_version {found} does not match engine ABI {expected}")]
    AbiVersion { found: u32, expected: u32 },
    /// Unknown scene space.
    #[error("unknown scene_space '{0}'")]
    UnknownSceneSpace(String),
    /// Path escapes package directory.
    #[error("path '{0}' must remain within the package directory")]
    PathEscapesPackage(String),
    /// Duration out of bounds.
    #[error(
        "display duration {0}s out of bounds [{MIN_DISPLAY_DURATION_SECONDS}, {MAX_DISPLAY_DURATION_SECONDS}]s"
    )]
    DurationSecondsOutOfBounds(u32),
    /// Invalid sidecar preopen.
    #[error("invalid sidecar preopen '{0}'")]
    InvalidSidecarPreopen(String),
}

impl SlideManifest {
    /// Validates the manifest.
    ///
    /// # Arguments
    /// * `abi_version` - The engine's ABI version to check against
    ///
    /// # Returns
    /// * `Ok(())` if validation passes
    /// * `Err(ManifestValidationError)` if validation fails
    pub fn validate(&self, abi_version: u32) -> Result<(), ManifestValidationError> {
        // Check ABI version
        if let Some(found) = self.abi_version {
            if found != abi_version {
                return Err(ManifestValidationError::AbiVersion {
                    found,
                    expected: abi_version,
                });
            }
        }

        // Check scene space
        if let Some(scene_space) = self.scene_space.as_deref() {
            if !matches!(scene_space, "screen_2d" | "world_3d") {
                return Err(ManifestValidationError::UnknownSceneSpace(
                    scene_space.to_string(),
                ));
            }
        }

        // Validate asset paths
        if let Some(assets) = &self.assets {
            for texture in &assets.textures {
                validate_package_relative_path(&texture.path)?;
            }
            for mesh in &assets.meshes {
                validate_package_relative_path(&mesh.path)?;
            }
            for scene in &assets.scenes {
                validate_package_relative_path(&scene.path)?;
            }
        }

        // Validate shader paths
        if let Some(shaders) = &self.shaders {
            if let Some(vertex) = shaders.vertex.as_deref() {
                validate_package_relative_path(vertex)?;
            }
            if let Some(fragment) = shaders.fragment.as_deref() {
                validate_package_relative_path(fragment)?;
            }
        }

        // Validate display duration
        if let Some(duration_seconds) = self.display_duration_seconds() {
            if !(MIN_DISPLAY_DURATION_SECONDS..=MAX_DISPLAY_DURATION_SECONDS)
                .contains(&duration_seconds)
            {
                return Err(ManifestValidationError::DurationSecondsOutOfBounds(
                    duration_seconds,
                ));
            }
        }

        // Validate sidecar preopens
        if let Some(sidecar) = &self.sidecar {
            for preopen in &sidecar.wasi_preopens {
                validate_sidecar_preopen(preopen)?;
            }
        }

        Ok(())
    }

    /// Returns the transition-in kind.
    pub fn transition_in_kind(&self) -> Option<TransitionKind> {
        self.display
            .as_ref()
            .and_then(|display| display.transition_in.as_deref())
            .map(parse_transition_kind)
    }

    /// Returns the transition-out kind.
    pub fn transition_out_kind(&self) -> Option<TransitionKind> {
        self.display
            .as_ref()
            .and_then(|display| display.transition_out.as_deref())
            .map(parse_transition_kind)
    }

    /// Returns the display duration in seconds.
    pub fn display_duration_seconds(&self) -> Option<u32> {
        self.display
            .as_ref()
            .and_then(|display| display.duration_seconds)
    }

    /// Returns a scene asset by ID, or the first scene if no ID is specified.
    pub fn scene_asset(&self, requested_id: Option<&str>) -> Option<&SceneAssetRef> {
        let assets = self.assets.as_ref()?;
        match requested_id {
            Some(id) => assets
                .scenes
                .iter()
                .find(|scene| scene.id.as_deref() == Some(id)),
            None => assets.scenes.first(),
        }
    }
}

/// Validates that a path is relative and doesn't escape the package directory.
fn validate_package_relative_path(path: &str) -> Result<(), ManifestValidationError> {
    let candidate = std::path::Path::new(path);
    for component in candidate.components() {
        match component {
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::ParentDir => {
                return Err(ManifestValidationError::PathEscapesPackage(
                    path.to_string(),
                ));
            }
            std::path::Component::CurDir | std::path::Component::Normal(_) => {}
        }
    }
    Ok(())
}

/// Validates a sidecar preopen specification.
fn validate_sidecar_preopen(spec: &str) -> Result<(), ManifestValidationError> {
    let Some((host, guest)) = spec.rsplit_once(':') else {
        return Err(ManifestValidationError::InvalidSidecarPreopen(
            spec.to_string(),
        ));
    };
    if host.is_empty() || guest.is_empty() {
        return Err(ManifestValidationError::InvalidSidecarPreopen(
            spec.to_string(),
        ));
    }
    if !std::path::Path::new(host).is_absolute()
        || !std::path::Path::new(guest).is_absolute()
    {
        return Err(ManifestValidationError::InvalidSidecarPreopen(
            spec.to_string(),
        ));
    }
    Ok(())
}

/// Parses a transition kind string.
pub fn parse_transition_kind(kind: &str) -> TransitionKind {
    match kind {
        "crossfade" => TransitionKind::Crossfade,
        "wipe_left" => TransitionKind::WipeLeft,
        "wipe_down" => TransitionKind::WipeDown,
        "dissolve" => TransitionKind::Dissolve,
        "cut" => TransitionKind::Cut,
        other => {
            // In kernel, we can't log, so we just return default
            TransitionKind::Crossfade
        }
    }
}

/// Parse a manifest from JSON bytes.
pub fn parse_manifest(json_bytes: &[u8]) -> Result<SlideManifest, String> {
    let content = std::str::from_utf8(json_bytes)
        .map_err(|e| format!("invalid UTF-8 in manifest: {e}"))?;
    serde_json::from_str(content).map_err(|e| format!("invalid manifest JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINE_ABI: u32 = 1;

    #[test]
    fn minimal_manifest_parses() {
        let json = br#"{"name":"Test"}"#;
        let manifest: SlideManifest = parse_manifest(json).expect("parse manifest");

        assert_eq!(manifest.name.as_deref(), Some("Test"));
        assert!(manifest.display.is_none());
        assert!(manifest.assets.is_none());
        assert!(manifest.shaders.is_none());
        assert!(manifest.sidecar.is_none());
    }

    #[test]
    fn invalid_abi_version_is_rejected() {
        let manifest = SlideManifest {
            abi_version: Some(99),
            ..Default::default()
        };

        let error = manifest
            .validate(ENGINE_ABI)
            .expect_err("abi mismatch should fail");

        assert_eq!(
            error,
            ManifestValidationError::AbiVersion {
                found: 99,
                expected: ENGINE_ABI,
            }
        );
    }

    #[test]
    fn unknown_scene_space_is_rejected() {
        let manifest = SlideManifest {
            scene_space: Some("isometric".into()),
            ..Default::default()
        };

        let error = manifest
            .validate(ENGINE_ABI)
            .expect_err("unknown scene space should fail");

        assert_eq!(
            error,
            ManifestValidationError::UnknownSceneSpace("isometric".into())
        );
    }

    #[test]
    fn asset_path_traversal_is_rejected() {
        let manifest = SlideManifest {
            assets: Some(ManifestAssets {
                textures: vec![AssetRef {
                    path: "../secret.png".into(),
                    usage: Some("material".into()),
                    slot: None,
                    label: None,
                    id: None,
                }],
                meshes: vec![],
                scenes: vec![],
            }),
            ..Default::default()
        };

        let error = manifest
            .validate(ENGINE_ABI)
            .expect_err("path traversal should fail");

        assert_eq!(
            error,
            ManifestValidationError::PathEscapesPackage("../secret.png".into())
        );
    }

    #[test]
    fn display_duration_out_of_bounds_is_rejected() {
        let manifest = SlideManifest {
            display: Some(DisplayConfig {
                duration_seconds: Some(301),
                transition_in: None,
                transition_out: None,
            }),
            ..Default::default()
        };

        let error = manifest
            .validate(ENGINE_ABI)
            .expect_err("duration should fail bounds check");

        assert_eq!(
            error,
            ManifestValidationError::DurationSecondsOutOfBounds(301)
        );
    }

    #[test]
    fn parse_transition_kind_valid() {
        assert_eq!(parse_transition_kind("crossfade"), TransitionKind::Crossfade);
        assert_eq!(parse_transition_kind("wipe_left"), TransitionKind::WipeLeft);
        assert_eq!(parse_transition_kind("wipe_down"), TransitionKind::WipeDown);
        assert_eq!(parse_transition_kind("dissolve"), TransitionKind::Dissolve);
        assert_eq!(parse_transition_kind("cut"), TransitionKind::Cut);
    }

    #[test]
    fn parse_transition_kind_unknown_returns_default() {
        assert_eq!(
            parse_transition_kind("mystery"),
            TransitionKind::Crossfade
        );
    }
}
