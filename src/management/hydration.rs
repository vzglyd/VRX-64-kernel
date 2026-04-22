//! Playlist hydration — combining playlist entries with their resolved manifest metadata.
//!
//! Both the native management server and the web editor (via wasm-bindgen exports)
//! use this shared logic so that validation rules and display defaults are never
//! duplicated between platforms.

use serde::{Deserialize, Serialize};

use crate::manifest::parse_transition_kind;
use crate::manifest::{ManifestParamType, ManifestParamsSchema, SlideManifest};
use crate::schedule::{PlaylistDefaults, PlaylistEntry};
use crate::transition::TransitionKind;

/// A playlist entry combined with its resolved manifest metadata and validation results.
///
/// Produced by [`hydrate_entry`] and consumed by management UIs to render
/// an editor card with correct defaults, param schema, and inline error messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydratedPlaylistEntry {
    /// The raw playlist entry as it appears in `playlist.json`.
    pub entry: PlaylistEntry,
    /// The manifest from the associated `.vzglyd` bundle, if it could be resolved.
    pub manifest: Option<SlideManifest>,
    /// Effective display duration in seconds after applying playlist and manifest defaults.
    pub resolved_duration_secs: f32,
    /// Effective transition-in after applying playlist and manifest defaults.
    pub resolved_transition_in: Option<TransitionKind>,
    /// Effective transition-out after applying playlist and manifest defaults.
    pub resolved_transition_out: Option<TransitionKind>,
    /// Human-readable validation errors for `entry.params` against `manifest.params`.
    /// An empty list means the params are valid (or no schema is available to check).
    pub param_errors: Vec<String>,
}

/// Engine default duration used when neither the entry, playlist defaults, nor
/// manifest specify a duration.
pub const ENGINE_DEFAULT_DURATION_SECS: f32 = 7.0;

/// Hydrate a single playlist entry against its manifest and playlist defaults.
///
/// This is the canonical resolution path shared by native and web. It resolves:
/// - Display duration (entry → defaults → manifest → engine default)
/// - Transitions (entry → defaults → manifest → `None`)
/// - Parameter validation errors
pub fn hydrate_entry(
    entry: &PlaylistEntry,
    manifest: Option<&SlideManifest>,
    defaults: &PlaylistDefaults,
    engine_default_duration: f32,
) -> HydratedPlaylistEntry {
    let resolved_duration_secs =
        resolve_duration(entry, defaults, manifest, engine_default_duration);
    let resolved_transition_in = resolve_transition_in(entry, defaults, manifest);
    let resolved_transition_out = resolve_transition_out(entry, defaults, manifest);
    let param_errors = validate_params(
        entry.params.as_ref(),
        manifest.and_then(|m| m.params.as_ref()),
    );

    HydratedPlaylistEntry {
        entry: entry.clone(),
        manifest: manifest.cloned(),
        resolved_duration_secs,
        resolved_transition_in,
        resolved_transition_out,
        param_errors,
    }
}

/// Validate that a params JSON value conforms to a manifest's param schema.
///
/// Returns a list of human-readable error messages. An empty list means the
/// params are valid (or no schema was provided to check against).
pub fn validate_params(
    params: Option<&serde_json::Value>,
    schema: Option<&ManifestParamsSchema>,
) -> Vec<String> {
    let Some(schema) = schema else {
        return Vec::new();
    };

    let mut errors = Vec::new();

    // Check required fields are present
    for field in &schema.fields {
        if !field.required {
            continue;
        }
        let value = params.and_then(|p| p.get(&field.key));
        if value.is_none() || value == Some(&serde_json::Value::Null) {
            errors.push(format!("required param '{}' is missing", field.key));
            continue;
        }
    }

    // Check supplied values match their declared types
    if let Some(params_obj) = params.and_then(|p| p.as_object()) {
        for (key, value) in params_obj {
            let Some(field) = schema.fields.iter().find(|f| &f.key == key) else {
                // Unknown params are allowed — slides may accept undeclared keys
                continue;
            };

            if !value_matches_type(value, field.kind) {
                errors.push(format!(
                    "param '{}' expected type '{}' but got {}",
                    key,
                    field.kind,
                    json_type_name(value),
                ));
                continue;
            }

            // Check value is in the allowed options list (if options are declared)
            if !field.options.is_empty() && !field.options.iter().any(|o| o.value == *value) {
                let allowed: Vec<String> = field
                    .options
                    .iter()
                    .map(|o| serde_json::to_string(&o.value).unwrap_or_default())
                    .collect();
                errors.push(format!(
                    "param '{}' value {} is not one of [{}]",
                    key,
                    serde_json::to_string(value).unwrap_or_default(),
                    allowed.join(", "),
                ));
            }
        }
    }

    errors
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn resolve_duration(
    entry: &PlaylistEntry,
    defaults: &PlaylistDefaults,
    manifest: Option<&SlideManifest>,
    engine_default: f32,
) -> f32 {
    // Priority: entry → playlist defaults → manifest display → engine default
    if let Some(d) = entry.duration_seconds {
        return d as f32;
    }
    if let Some(d) = defaults.duration_seconds {
        return d as f32;
    }
    if let Some(d) = manifest.and_then(|m| m.display_duration_seconds()) {
        return d as f32;
    }
    engine_default
}

fn resolve_transition_in(
    entry: &PlaylistEntry,
    defaults: &PlaylistDefaults,
    manifest: Option<&SlideManifest>,
) -> Option<TransitionKind> {
    entry
        .transition_in
        .as_deref()
        .or(defaults.transition_in.as_deref())
        .or_else(|| {
            manifest.and_then(|m| m.display.as_ref().and_then(|d| d.transition_in.as_deref()))
        })
        .map(parse_transition_kind)
}

fn resolve_transition_out(
    entry: &PlaylistEntry,
    defaults: &PlaylistDefaults,
    manifest: Option<&SlideManifest>,
) -> Option<TransitionKind> {
    entry
        .transition_out
        .as_deref()
        .or(defaults.transition_out.as_deref())
        .or_else(|| {
            manifest.and_then(|m| m.display.as_ref().and_then(|d| d.transition_out.as_deref()))
        })
        .map(parse_transition_kind)
}

fn value_matches_type(value: &serde_json::Value, kind: ManifestParamType) -> bool {
    match kind {
        ManifestParamType::String => value.is_string(),
        ManifestParamType::Integer => value.is_i64() || value.is_u64(),
        ManifestParamType::Number => value.is_number(),
        ManifestParamType::Boolean => value.is_boolean(),
        ManifestParamType::Json => true,
    }
}

fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{DisplayConfig, ManifestParamField, ManifestParamOption};
    use crate::schedule::PlaylistDefaults;

    fn entry_with_path(path: &str) -> PlaylistEntry {
        PlaylistEntry {
            path: path.into(),
            ..Default::default()
        }
    }

    fn defaults_with_duration(secs: u32) -> PlaylistDefaults {
        PlaylistDefaults {
            duration_seconds: Some(secs),
            ..Default::default()
        }
    }

    #[test]
    fn hydrate_entry_resolves_duration_from_entry() {
        let entry = PlaylistEntry {
            path: "test.vzglyd".into(),
            duration_seconds: Some(15),
            ..Default::default()
        };
        let result = hydrate_entry(&entry, None, &PlaylistDefaults::default(), 7.0);
        assert_eq!(result.resolved_duration_secs, 15.0);
    }

    #[test]
    fn hydrate_entry_falls_through_to_defaults() {
        let entry = entry_with_path("test.vzglyd");
        let defaults = defaults_with_duration(12);
        let result = hydrate_entry(&entry, None, &defaults, 7.0);
        assert_eq!(result.resolved_duration_secs, 12.0);
    }

    #[test]
    fn hydrate_entry_falls_through_to_manifest() {
        let entry = entry_with_path("test.vzglyd");
        let manifest = SlideManifest {
            display: Some(DisplayConfig {
                duration_seconds: Some(20),
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = hydrate_entry(&entry, Some(&manifest), &PlaylistDefaults::default(), 7.0);
        assert_eq!(result.resolved_duration_secs, 20.0);
    }

    #[test]
    fn hydrate_entry_falls_through_to_engine_default() {
        let entry = entry_with_path("test.vzglyd");
        let result = hydrate_entry(&entry, None, &PlaylistDefaults::default(), 7.0);
        assert_eq!(result.resolved_duration_secs, 7.0);
    }

    #[test]
    fn validate_params_no_schema_always_ok() {
        let params = serde_json::json!({"anything": "goes"});
        let errors = validate_params(Some(&params), None);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_params_missing_required_field() {
        let schema = ManifestParamsSchema {
            fields: vec![ManifestParamField {
                key: "api_key".into(),
                kind: ManifestParamType::String,
                required: true,
                label: None,
                help: None,
                default: None,
                options: vec![],
            }],
        };
        let errors = validate_params(None, Some(&schema));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("api_key"));
    }

    #[test]
    fn validate_params_wrong_type() {
        let schema = ManifestParamsSchema {
            fields: vec![ManifestParamField {
                key: "count".into(),
                kind: ManifestParamType::Integer,
                required: false,
                label: None,
                help: None,
                default: None,
                options: vec![],
            }],
        };
        let params = serde_json::json!({"count": "not_a_number"});
        let errors = validate_params(Some(&params), Some(&schema));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("count"));
    }

    #[test]
    fn validate_params_invalid_option_value() {
        let schema = ManifestParamsSchema {
            fields: vec![ManifestParamField {
                key: "mode".into(),
                kind: ManifestParamType::String,
                required: false,
                label: None,
                help: None,
                default: None,
                options: vec![
                    ManifestParamOption {
                        value: serde_json::json!("light"),
                        label: None,
                    },
                    ManifestParamOption {
                        value: serde_json::json!("dark"),
                        label: None,
                    },
                ],
            }],
        };
        let params = serde_json::json!({"mode": "unknown"});
        let errors = validate_params(Some(&params), Some(&schema));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("mode"));
    }

    #[test]
    fn validate_params_valid_passes() {
        let schema = ManifestParamsSchema {
            fields: vec![ManifestParamField {
                key: "timezone".into(),
                kind: ManifestParamType::String,
                required: true,
                label: None,
                help: None,
                default: None,
                options: vec![],
            }],
        };
        let params = serde_json::json!({"timezone": "UTC"});
        let errors = validate_params(Some(&params), Some(&schema));
        assert!(errors.is_empty());
    }
}
