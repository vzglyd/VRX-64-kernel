//! Schedule and playlist management.
//!
//! This module handles playlist parsing and slide schedule construction.

use serde::{Deserialize, Serialize};

use crate::manifest::parse_transition_kind;
use crate::transition::TransitionKind;

/// Filename for playlist configuration.
pub const PLAYLIST_FILENAME: &str = "playlist.json";

/// Top-level structure for `playlist.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    /// Default display settings applied to any entry that does not specify its own.
    #[serde(default)]
    pub defaults: PlaylistDefaults,
    /// Ordered list of slides to display.
    pub slides: Vec<PlaylistEntry>,
    /// Output scale factor applied when blitting slides to the display surface.
    ///
    /// `1.0` (default) fills the letterbox rect exactly.
    /// Values below `1.0` shrink the output and add black bars — useful on CRT
    /// displays where the bezel crops the outermost pixels (overscan).
    /// Values above `1.0` zoom in, cropping the edges.
    #[serde(default = "default_display_scale")]
    pub display_scale: f32,
}

fn default_display_scale() -> f32 {
    1.0
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            defaults: PlaylistDefaults::default(),
            slides: Vec::new(),
            display_scale: 1.0,
        }
    }
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
    /// Optional screensaver configuration. When present, activates a burn-in protection
    /// intermission after the display has been running for `timeout_seconds`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screensaver: Option<ScreensaverConfig>,
}

/// Screensaver / burn-in protection configuration.
///
/// After `timeout_seconds` of continuous display the normal playlist and border
/// overlay are suppressed. A full-screen "Intermission" scene with a drifting
/// countdown is shown instead. When `duration_seconds` elapses the playlist
/// resumes from where it left off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreensaverConfig {
    /// Seconds of continuous display before the screensaver activates. Default: 300 (5 min).
    pub timeout_seconds: u32,
    /// How long (seconds) the screensaver runs before resuming the playlist. Default: 60.
    pub duration_seconds: u32,
}

impl Default for ScreensaverConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 300,
            duration_seconds: 60,
        }
    }
}

/// A single entry in the `slides` array of `playlist.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaylistEntry {
    /// Path to the `.vzglyd` archive or slide directory, relative to the slides directory.
    pub path: String,
    /// Optional JSON result file watched by the host for this slide's live data.
    ///
    /// Relative paths resolve from the slides repository root. Absolute paths
    /// are preserved unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
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
    /// Absolute or repo-root-resolved path to the watched JSON data source.
    pub data_path: Option<String>,
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
    let playlist: Playlist =
        serde_json::from_str(content).map_err(|e| format!("invalid playlist JSON: {e}"))?;
    validate_playlist(&playlist)?;
    Ok(playlist)
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
            data_path: entry
                .data_path
                .as_deref()
                .map(|path| resolve_data_path(base_path, path)),
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

fn resolve_data_path(base_path: &str, path: &str) -> String {
    let candidate = std::path::Path::new(path);
    if candidate.is_absolute() {
        return path.to_string();
    }

    std::path::Path::new(base_path)
        .join(candidate)
        .to_string_lossy()
        .into_owned()
}

fn validate_playlist(playlist: &Playlist) -> Result<(), String> {
    for (index, entry) in playlist.slides.iter().enumerate() {
        if let Some(path) = entry.data_path.as_deref() {
            validate_data_path(path)
                .map_err(|error| format!("slides[{index}].data_path {error}"))?;
        }
    }
    Ok(())
}

fn validate_data_path(path: &str) -> Result<(), &'static str> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("must be a non-empty string");
    }
    if trimmed.contains('\\') {
        return Err("must use forward slashes");
    }

    let candidate = std::path::Path::new(trimmed);
    if !candidate.is_absolute()
        && trimmed
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err("must not contain . or .. segments");
    }

    Ok(())
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
            display_scale: 1.0,
            slides: vec![
                PlaylistEntry {
                    path: "a.vzglyd".into(),
                    data_path: None,
                    enabled: Some(true),
                    duration_seconds: None,
                    transition_in: None,
                    transition_out: None,
                    params: None,
                },
                PlaylistEntry {
                    path: "b.vzglyd".into(),
                    data_path: None,
                    enabled: Some(false),
                    duration_seconds: None,
                    transition_in: None,
                    transition_out: None,
                    params: None,
                },
                PlaylistEntry {
                    path: "c.vzglyd".into(),
                    data_path: None,
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
                screensaver: None,
            },
            display_scale: 1.0,
            slides: vec![PlaylistEntry {
                path: "clock.vzglyd".into(),
                data_path: Some("data/weather.out.json".into()),
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
        assert_eq!(
            resolved[0].data_path.as_deref(),
            Some("slides/data/weather.out.json")
        );
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
            screensaver: None,
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
    fn resolve_schedule_preserves_absolute_data_path() {
        let playlist = Playlist {
            defaults: PlaylistDefaults::default(),
            display_scale: 1.0,
            slides: vec![PlaylistEntry {
                path: "clock.vzglyd".into(),
                data_path: Some("/tmp/weather.out.json".into()),
                enabled: Some(true),
                duration_seconds: None,
                transition_in: None,
                transition_out: None,
                params: None,
            }],
        };

        let resolved = resolve_schedule_from_playlist(&playlist, "slides", 7.0);
        assert_eq!(
            resolved[0].data_path.as_deref(),
            Some("/tmp/weather.out.json")
        );
    }

    #[test]
    fn parse_screensaver_config() {
        let json = br#"{
            "defaults": {
                "duration_seconds": 10,
                "screensaver": { "timeout_seconds": 300, "duration_seconds": 60 }
            },
            "slides": [{ "path": "clock.vzglyd" }]
        }"#;
        let playlist = parse_playlist(json).expect("parse playlist");
        let ss = playlist.defaults.screensaver.expect("screensaver config");
        assert_eq!(ss.timeout_seconds, 300);
        assert_eq!(ss.duration_seconds, 60);
    }

    #[test]
    fn screensaver_config_is_optional() {
        let json = br#"{"slides":[{"path":"clock.vzglyd"}]}"#;
        let playlist = parse_playlist(json).expect("parse playlist");
        assert!(playlist.defaults.screensaver.is_none());
    }

    #[test]
    fn parse_invalid_json() {
        let json = b"not json";
        assert!(parse_playlist(json).is_err());
    }

    #[test]
    fn parse_playlist_rejects_empty_data_path() {
        let json = br#"{"slides":[{"path":"clock.vzglyd","data_path":"   "}]} "#;
        let error = parse_playlist(json).expect_err("empty data_path should fail");
        assert!(error.contains("slides[0].data_path must be a non-empty string"));
    }

    #[test]
    fn parse_playlist_rejects_relative_data_path_escape() {
        let json = br#"{"slides":[{"path":"clock.vzglyd","data_path":"../weather.out.json"}]}"#;
        let error = parse_playlist(json).expect_err("escaped data_path should fail");
        assert!(error.contains("slides[0].data_path must not contain . or .. segments"));
    }

    #[test]
    fn parse_playlist_accepts_absolute_data_path() {
        let json = br#"{"slides":[{"path":"clock.vzglyd","data_path":"/tmp/weather.out.json"}]}"#;
        let playlist = parse_playlist(json).expect("absolute data_path should parse");
        assert_eq!(
            playlist.slides[0].data_path.as_deref(),
            Some("/tmp/weather.out.json")
        );
    }
}
