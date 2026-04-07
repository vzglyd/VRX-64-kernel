//! Schedule and playlist management.
//!
//! This module handles playlist parsing and slide schedule construction.

use serde::{Deserialize, Serialize};

use crate::manifest::parse_transition_kind;
use crate::transition::TransitionKind;

/// Filename for playlist configuration.
pub const PLAYLIST_FILENAME: &str = "playlist.json";

/// Top-level structure for `playlist.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Playlist {
    /// Default display settings applied to any entry that does not specify its own.
    #[serde(default)]
    pub defaults: PlaylistDefaults,
    /// Ordered list of slides to display.
    pub slides: Vec<PlaylistEntry>,
}

/// Fallback display settings for entries that do not override them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaylistDefaults {
    /// How long each slide is shown (seconds). Overrides the engine default; overridden by per-entry value.
    pub duration_seconds: Option<u32>,
    /// Transition played when this slide enters the screen.
    pub transition_in: Option<String>,
    /// Transition played when this slide leaves the screen.
    pub transition_out: Option<String>,
}

/// A single entry in the `slides` array of `playlist.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaylistEntry {
    /// Path to the `.vzglyd` archive or slide directory, relative to the slides directory.
    pub path: String,
    /// Set to `false` to skip this slide without removing it from the file. Absent means `true`.
    pub enabled: Option<bool>,
    /// Override display duration for this slide (seconds).
    pub duration_seconds: Option<u32>,
    /// Override transition-in for this slide.
    pub transition_in: Option<String>,
    /// Override transition-out for this slide.
    pub transition_out: Option<String>,
    /// Optional JSON parameters written to the slide's configure buffer before init.
    pub params: Option<serde_json::Value>,
}

/// A fully resolved slide entry produced from a playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSlideEntry {
    /// Path to the slide package.
    pub path: String,
    /// Effective duration in seconds after playlist/default resolution.
    pub duration_secs: f32,
    /// Transition played when this slide enters the screen.
    pub transition_in: Option<TransitionKind>,
    /// Transition played when this slide leaves the screen.
    pub transition_out: Option<TransitionKind>,
    /// Optional JSON parameters written into the slide configure buffer.
    pub params: Option<serde_json::Value>,
}

impl PlaylistEntry {
    /// Returns true if this entry is enabled.
    ///
    /// An entry is enabled if the `enabled` field is `None` or `Some(true)`.
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

/// Parse a playlist from JSON bytes.
///
/// # Arguments
/// * `json_bytes` - The JSON content of the playlist file
///
/// # Returns
/// * `Ok(Playlist)` if parsing succeeds
/// * `Err` if the JSON is invalid
pub fn parse_playlist(json_bytes: &[u8]) -> Result<Playlist, String> {
    let content =
        std::str::from_utf8(json_bytes).map_err(|e| format!("invalid UTF-8 in playlist: {e}"))?;
    serde_json::from_str(content).map_err(|e| format!("invalid playlist JSON: {e}"))
}

/// Build a schedule from a playlist.
///
/// # Arguments
/// * `playlist` - The parsed playlist
/// * `base_path` - Base path to prepend to each entry's path
///
/// # Returns
/// A vector of slide paths (filtered to only enabled entries)
pub fn build_schedule_from_playlist(playlist: &Playlist, base_path: &str) -> Vec<String> {
    resolve_schedule_from_playlist(playlist, base_path, 7.0)
        .into_iter()
        .map(|entry| entry.path)
        .collect()
}

/// Resolve a playlist into fully described schedule entries.
pub fn resolve_schedule_from_playlist(
    playlist: &Playlist,
    base_path: &str,
    engine_default_duration: f32,
) -> Vec<ResolvedSlideEntry> {
    playlist
        .slides
        .iter()
        .filter(|entry| entry.is_enabled())
        .map(|entry| ResolvedSlideEntry {
            path: if base_path.ends_with('/') {
                format!("{}{}", base_path, entry.path)
            } else {
                format!("{}/{}", base_path, entry.path)
            },
            duration_secs: resolve_duration(entry, &playlist.defaults, engine_default_duration),
            transition_in: entry
                .transition_in
                .as_deref()
                .or(playlist.defaults.transition_in.as_deref())
                .map(parse_transition_kind),
            transition_out: entry
                .transition_out
                .as_deref()
                .or(playlist.defaults.transition_out.as_deref())
                .map(parse_transition_kind),
            params: entry.params.clone(),
        })
        .collect()
}

/// Resolve the duration for a slide entry.
///
/// Priority order:
/// 1. Entry-specific duration
/// 2. Playlist default duration
/// 3. Engine default duration
///
/// # Arguments
/// * `entry` - The playlist entry
/// * `defaults` - Playlist defaults
/// * `engine_default` - Engine default duration
pub fn resolve_duration(
    entry: &PlaylistEntry,
    defaults: &PlaylistDefaults,
    engine_default: f32,
) -> f32 {
    entry
        .duration_seconds
        .map(|s| s as f32)
        .or(defaults.duration_seconds.map(|s| s as f32))
        .unwrap_or(engine_default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_playlist() {
        let json = br#"{"slides":[{"path":"clock.vzglyd"}]}"#;
        let playlist = parse_playlist(json).expect("parse playlist");

        assert_eq!(playlist.slides.len(), 1);
        assert_eq!(playlist.slides[0].path, "clock.vzglyd");
        assert!(playlist.slides[0].enabled.is_none());
        assert!(playlist.slides[0].duration_seconds.is_none());
    }

    #[test]
    fn parse_full_playlist() {
        let json = br#"{
            "defaults": { "duration_seconds": 10, "transition_in": "crossfade" },
            "slides": [
                { "path": "a.vzglyd", "duration_seconds": 20, "transition_out": "cut" },
                { "path": "b.vzglyd", "enabled": false }
            ]
        }"#;

        let playlist = parse_playlist(json).expect("parse playlist");
        assert_eq!(playlist.defaults.duration_seconds, Some(10));
        assert_eq!(
            playlist.defaults.transition_in.as_deref(),
            Some("crossfade")
        );
        assert_eq!(playlist.slides[0].duration_seconds, Some(20));
        assert_eq!(playlist.slides[0].transition_out.as_deref(), Some("cut"));
        assert_eq!(playlist.slides[1].enabled, Some(false));
    }

    #[test]
    fn build_schedule_filters_disabled() {
        let playlist = Playlist {
            defaults: PlaylistDefaults::default(),
            slides: vec![
                PlaylistEntry {
                    path: "a.vzglyd".into(),
                    enabled: Some(true),
                    duration_seconds: None,
                    transition_in: None,
                    transition_out: None,
                    params: None,
                },
                PlaylistEntry {
                    path: "b.vzglyd".into(),
                    enabled: Some(false),
                    duration_seconds: None,
                    transition_in: None,
                    transition_out: None,
                    params: None,
                },
                PlaylistEntry {
                    path: "c.vzglyd".into(),
                    enabled: None, // Default: enabled
                    duration_seconds: None,
                    transition_in: None,
                    transition_out: None,
                    params: None,
                },
            ],
        };

        let schedule = build_schedule_from_playlist(&playlist, "slides/");
        assert_eq!(schedule.len(), 2);
        assert_eq!(schedule[0], "slides/a.vzglyd");
        assert_eq!(schedule[1], "slides/c.vzglyd");
    }

    #[test]
    fn resolve_schedule_keeps_overrides_and_params() {
        let playlist = Playlist {
            defaults: PlaylistDefaults {
                duration_seconds: Some(10),
                transition_in: Some("crossfade".into()),
                transition_out: Some("wipe_left".into()),
            },
            slides: vec![PlaylistEntry {
                path: "clock.vzglyd".into(),
                enabled: Some(true),
                duration_seconds: Some(20),
                transition_in: None,
                transition_out: Some("cut".into()),
                params: Some(serde_json::json!({"mode":"demo"})),
            }],
        };

        let resolved = resolve_schedule_from_playlist(&playlist, "slides", 7.0);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].path, "slides/clock.vzglyd");
        assert_eq!(resolved[0].duration_secs, 20.0);
        assert_eq!(resolved[0].transition_in, Some(TransitionKind::Crossfade));
        assert_eq!(resolved[0].transition_out, Some(TransitionKind::Cut));
        assert_eq!(resolved[0].params, Some(serde_json::json!({"mode":"demo"})));
    }

    #[test]
    fn resolve_duration_priority() {
        let defaults = PlaylistDefaults {
            duration_seconds: Some(10),
            transition_in: None,
            transition_out: None,
        };

        // Entry override takes priority
        let entry = PlaylistEntry {
            duration_seconds: Some(20),
            ..Default::default()
        };
        assert_eq!(resolve_duration(&entry, &defaults, 7.0), 20.0);

        // Falls back to default
        let entry = PlaylistEntry {
            duration_seconds: None,
            ..Default::default()
        };
        assert_eq!(resolve_duration(&entry, &defaults, 7.0), 10.0);

        // Falls back to engine default
        let entry = PlaylistEntry {
            duration_seconds: None,
            ..Default::default()
        };
        let defaults = PlaylistDefaults::default();
        assert_eq!(resolve_duration(&entry, &defaults, 7.0), 7.0);
    }

    #[test]
    fn parse_invalid_json() {
        let json = b"not json";
        assert!(parse_playlist(json).is_err());
    }
}
