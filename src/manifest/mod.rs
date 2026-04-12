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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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
    /// Parameter schema for runtime customisation.
    pub params: Option<ManifestParamsSchema>,
}

/// Asset references in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ManifestAssets {
    /// Cassette artwork used by management tools and bundle libraries.
    #[serde(default)]
    pub art: Option<ManifestCassetteArt>,
    /// Texture assets.
    #[serde(default)]
    pub textures: Vec<AssetRef>,
    /// Mesh assets.
    #[serde(default)]
    pub meshes: Vec<AssetRef>,
    /// Scene assets.
    #[serde(default)]
    pub scenes: Vec<SceneAssetRef>,
    /// Sound assets (MP3, WAV, Ogg, FLAC).
    #[serde(default)]
    pub sounds: Vec<SoundAssetRef>,
}

/// Required cassette artwork for a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestCassetteArt {
    /// J-card cassette cover image.
    pub j_card: ArtAssetRef,
    /// Tape label image for side A.
    pub side_a_label: ArtAssetRef,
    /// Tape label image for side B.
    pub side_b_label: ArtAssetRef,
}

/// Reference to a bundle artwork image.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtAssetRef {
    /// Path to the artwork image file.
    pub path: String,
    /// Human-readable label for UI.
    #[serde(default)]
    pub label: Option<String>,
}

/// Reference to a sound asset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoundAssetRef {
    /// Path to the sound file.
    pub path: String,
    /// Audio format hint.
    #[serde(default)]
    pub format: Option<String>,
    /// Label for the asset.
    #[serde(default)]
    pub label: Option<String>,
    /// Unique ID for the asset.
    #[serde(default)]
    pub id: Option<String>,
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

/// Parameter schema declared in a manifest.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ManifestParamsSchema {
    /// Parameter field definitions.
    #[serde(default)]
    pub fields: Vec<ManifestParamField>,
}

/// A single parameter field in the manifest params schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestParamField {
    /// Unique key used to identify the parameter.
    pub key: String,
    /// Value type.
    #[serde(rename = "type")]
    pub kind: ManifestParamType,
    /// Whether the parameter must be supplied.
    #[serde(default)]
    pub required: bool,
    /// Human-readable label for UI.
    pub label: Option<String>,
    /// Help text for UI.
    pub help: Option<String>,
    /// Default value (must match `kind`).
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// Allowed values (empty means unrestricted).
    #[serde(default)]
    pub options: Vec<ManifestParamOption>,
}

/// One entry in a parameter field's option list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestParamOption {
    /// The option value (must match the field's `kind`).
    pub value: serde_json::Value,
    /// Human-readable label for UI.
    pub label: Option<String>,
}

/// Supported parameter value types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestParamType {
    /// UTF-8 string value.
    String,
    /// Integer numeric value.
    Integer,
    /// Floating-point numeric value.
    Number,
    /// Boolean value.
    Boolean,
    /// Arbitrary JSON value.
    Json,
}

impl std::fmt::Display for ManifestParamType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Json => "json",
        };
        write!(f, "{s}")
    }
}

/// Manifest validation errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestValidationError {
    /// ABI version mismatch.
    #[error("abi_version {found} does not match engine ABI {expected}")]
    AbiVersion {
        /// The ABI version found in the manifest.
        found: u32,
        /// The expected ABI version from the engine.
        expected: u32,
    },
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
    /// Invalid params schema.
    #[error("{0}")]
    InvalidParamsSchema(String),
    /// Invalid or missing cassette artwork.
    #[error("{0}")]
    InvalidCassetteArt(String),
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
            for sound in &assets.sounds {
                validate_package_relative_path(&sound.path)?;
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

        // Validate params schema
        if let Some(params) = &self.params {
            validate_params_schema(params)?;
        }

        // Validate required cassette art
        validate_cassette_art(self)?;

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

fn validate_cassette_art(manifest: &SlideManifest) -> Result<(), ManifestValidationError> {
    let Some(assets) = manifest.assets.as_ref() else {
        return Err(ManifestValidationError::InvalidCassetteArt(
            "manifest.assets.art is required".to_string(),
        ));
    };
    let Some(art) = assets.art.as_ref() else {
        return Err(ManifestValidationError::InvalidCassetteArt(
            "manifest.assets.art is required".to_string(),
        ));
    };

    validate_art_asset_ref("manifest.assets.art.j_card", &art.j_card)?;
    validate_art_asset_ref("manifest.assets.art.side_a_label", &art.side_a_label)?;
    validate_art_asset_ref("manifest.assets.art.side_b_label", &art.side_b_label)?;
    Ok(())
}

fn validate_art_asset_ref(label: &str, asset: &ArtAssetRef) -> Result<(), ManifestValidationError> {
    if asset.path.trim().is_empty() {
        return Err(ManifestValidationError::InvalidCassetteArt(format!(
            "{label}.path must be a non-empty string"
        )));
    }
    validate_package_relative_path(&asset.path)?;
    if let Some(label_text) = &asset.label {
        if label_text.trim().is_empty() {
            return Err(ManifestValidationError::InvalidCassetteArt(format!(
                "{label}.label must not be blank"
            )));
        }
    }
    Ok(())
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
    if !std::path::Path::new(host).is_absolute() || !std::path::Path::new(guest).is_absolute() {
        return Err(ManifestValidationError::InvalidSidecarPreopen(
            spec.to_string(),
        ));
    }
    Ok(())
}

/// Validates the params schema, checking for duplicate keys and type consistency.
fn validate_params_schema(schema: &ManifestParamsSchema) -> Result<(), ManifestValidationError> {
    let mut seen_keys = std::collections::BTreeSet::new();

    for field in &schema.fields {
        let key = field.key.trim();
        if key.is_empty() {
            return Err(ManifestValidationError::InvalidParamsSchema(
                "manifest.params.fields[].key must be a non-empty string".to_string(),
            ));
        }

        if !seen_keys.insert(key.to_string()) {
            return Err(ManifestValidationError::InvalidParamsSchema(format!(
                "manifest.params.fields contains duplicate key '{key}'"
            )));
        }

        if let Some(label) = &field.label {
            if label.trim().is_empty() {
                return Err(ManifestValidationError::InvalidParamsSchema(format!(
                    "manifest.params.fields['{key}'].label must not be blank"
                )));
            }
        }

        if let Some(help) = &field.help {
            if help.trim().is_empty() {
                return Err(ManifestValidationError::InvalidParamsSchema(format!(
                    "manifest.params.fields['{key}'].help must not be blank"
                )));
            }
        }

        if let Some(default) = &field.default {
            validate_param_value(
                default,
                field.kind,
                &format!("manifest.params.fields['{key}'].default"),
            )?;
        }

        if matches!(field.kind, ManifestParamType::Json) && !field.options.is_empty() {
            return Err(ManifestValidationError::InvalidParamsSchema(format!(
                "manifest.params.fields['{key}'].options are not supported for json fields"
            )));
        }

        let mut seen_options = std::collections::BTreeSet::new();
        for (index, option) in field.options.iter().enumerate() {
            validate_param_value(
                &option.value,
                field.kind,
                &format!("manifest.params.fields['{key}'].options[{index}].value"),
            )?;

            if let Some(label) = &option.label {
                if label.trim().is_empty() {
                    return Err(ManifestValidationError::InvalidParamsSchema(format!(
                        "manifest.params.fields['{key}'].options[{index}].label must not be blank"
                    )));
                }
            }

            let option_key =
                serde_json::to_string(&option.value).map_err(|e| {
                    ManifestValidationError::InvalidParamsSchema(format!(
                        "manifest.params.fields['{key}'].options[{index}].value could not be serialized: {e}"
                    ))
                })?;
            if !seen_options.insert(option_key) {
                return Err(ManifestValidationError::InvalidParamsSchema(format!(
                    "manifest.params.fields['{key}'].options contains duplicate values"
                )));
            }
        }

        if let Some(default) = &field.default {
            if !field.options.is_empty()
                && !field.options.iter().any(|o| o.value == *default)
            {
                return Err(ManifestValidationError::InvalidParamsSchema(format!(
                    "manifest.params.fields['{key}'].default must match one of the declared options"
                )));
            }
        }
    }

    Ok(())
}

fn validate_param_value(
    value: &serde_json::Value,
    kind: ManifestParamType,
    label: &str,
) -> Result<(), ManifestValidationError> {
    let is_valid = match kind {
        ManifestParamType::String => matches!(value, serde_json::Value::String(_)),
        ManifestParamType::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
        ManifestParamType::Number => value.as_f64().is_some(),
        ManifestParamType::Boolean => matches!(value, serde_json::Value::Bool(_)),
        ManifestParamType::Json => true,
    };

    if is_valid {
        Ok(())
    } else {
        Err(ManifestValidationError::InvalidParamsSchema(format!(
            "{label} does not match field type '{kind}'"
        )))
    }
}

/// Parses a transition kind string.
pub fn parse_transition_kind(kind: &str) -> TransitionKind {
    match kind {
        "crossfade" => TransitionKind::Crossfade,
        "wipe_left" => TransitionKind::WipeLeft,
        "wipe_down" => TransitionKind::WipeDown,
        "dissolve" => TransitionKind::Dissolve,
        "cut" => TransitionKind::Cut,
        _other => {
            // In kernel, we can't log, so we just return default
            TransitionKind::Crossfade
        }
    }
}

/// Parse a manifest from JSON bytes.
pub fn parse_manifest(json_bytes: &[u8]) -> Result<SlideManifest, String> {
    let content =
        std::str::from_utf8(json_bytes).map_err(|e| format!("invalid UTF-8 in manifest: {e}"))?;
    serde_json::from_str(content).map_err(|e| format!("invalid manifest JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINE_ABI: u32 = 1;

    fn required_art() -> ManifestCassetteArt {
        ManifestCassetteArt {
            j_card: ArtAssetRef {
                path: "art/j-card.png".into(),
                label: None,
            },
            side_a_label: ArtAssetRef {
                path: "art/side-a.png".into(),
                label: None,
            },
            side_b_label: ArtAssetRef {
                path: "art/side-b.png".into(),
                label: None,
            },
        }
    }

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
                art: Some(required_art()),
                textures: vec![AssetRef {
                    path: "../secret.png".into(),
                    usage: Some("material".into()),
                    slot: None,
                    label: None,
                    id: None,
                }],
                meshes: vec![],
                scenes: vec![],
                sounds: vec![],
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
    fn cassette_art_is_required() {
        let manifest = SlideManifest {
            assets: Some(ManifestAssets {
                art: None,
                textures: vec![],
                meshes: vec![],
                scenes: vec![],
                sounds: vec![],
            }),
            ..Default::default()
        };

        let error = manifest
            .validate(ENGINE_ABI)
            .expect_err("missing cassette art should fail");

        assert_eq!(
            error,
            ManifestValidationError::InvalidCassetteArt("manifest.assets.art is required".into())
        );
    }

    #[test]
    fn cassette_art_paths_are_validated() {
        let mut art = required_art();
        art.side_b_label.path = "../side-b.png".into();
        let manifest = SlideManifest {
            assets: Some(ManifestAssets {
                art: Some(art),
                textures: vec![],
                meshes: vec![],
                scenes: vec![],
                sounds: vec![],
            }),
            ..Default::default()
        };

        let error = manifest
            .validate(ENGINE_ABI)
            .expect_err("unsafe cassette art path should fail");

        assert_eq!(
            error,
            ManifestValidationError::PathEscapesPackage("../side-b.png".into())
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
        assert_eq!(
            parse_transition_kind("crossfade"),
            TransitionKind::Crossfade
        );
        assert_eq!(parse_transition_kind("wipe_left"), TransitionKind::WipeLeft);
        assert_eq!(parse_transition_kind("wipe_down"), TransitionKind::WipeDown);
        assert_eq!(parse_transition_kind("dissolve"), TransitionKind::Dissolve);
        assert_eq!(parse_transition_kind("cut"), TransitionKind::Cut);
    }

    #[test]
    fn parse_transition_kind_unknown_returns_default() {
        assert_eq!(parse_transition_kind("mystery"), TransitionKind::Crossfade);
    }
}
