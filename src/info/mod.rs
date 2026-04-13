//! Information slide state machine.
//!
//! The kernel tracks when an information slide should be displayed instead of
//! the normal playlist. This covers cold-start errors (missing playlist.json,
//! no valid slides) and situational alerts that require user intervention.
//!
//! The host renders the info slide; the kernel decides *when* to show it and
//! detects when normal operation can resume.

use std::path::Path;

/// The reason the information slide is being shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfoReason {
    /// No playlist.json found in the slides directory.
    MissingPlaylist {
        /// Path to the slides directory (shown so user knows where to put playlist.json).
        slides_dir: String,
        /// Management console URL the user should visit.
        management_url: String,
    },
    /// Playlist.json exists but is invalid / unparseable.
    InvalidPlaylist {
        /// The parse error message.
        error: String,
        /// Management console URL.
        management_url: String,
    },
    /// Playlist exists but contains zero enabled slides.
    EmptyPlaylist {
        /// Management console URL.
        management_url: String,
    },
    /// A general alert message — set by the host for situational errors.
    Alert {
        /// Title line shown prominently.
        title: String,
        /// Supporting detail lines.
        lines: Vec<String>,
    },
}

impl InfoReason {
    /// Returns the primary message line shown to the user.
    pub fn primary_message(&self) -> String {
        match self {
            Self::MissingPlaylist { slides_dir, .. } => {
                format!("No playlist.json found in '{}'", slides_dir)
            }
            Self::InvalidPlaylist { error, .. } => {
                format!("Invalid playlist.json: {}", error)
            }
            Self::EmptyPlaylist { .. } => {
                "No enabled slides in playlist".to_string()
            }
            Self::Alert { title, .. } => title.clone(),
        }
    }

    /// Returns secondary detail lines (may be empty).
    pub fn detail_lines(&self) -> Vec<String> {
        match self {
            Self::MissingPlaylist { management_url, .. }
            | Self::InvalidPlaylist { management_url, .. }
            | Self::EmptyPlaylist { management_url } => {
                vec![format!("Open management console: {}", management_url)]
            }
            Self::Alert { lines, .. } => lines.clone(),
        }
    }
}

/// State machine for the information slide.
#[derive(Debug, Clone, Default)]
pub struct InfoState {
    /// Why the info slide is currently shown. `None` means normal operation.
    pub reason: Option<InfoReason>,
}

impl InfoState {
    /// Create a new inactive info state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the info state to show an alert.
    pub fn show(&mut self, reason: InfoReason) {
        self.reason = Some(reason);
    }

    /// Clear the info state — normal playlist operation resumes.
    pub fn clear(&mut self) {
        self.reason = None;
    }

    /// Returns `true` if the info slide should be displayed.
    pub fn is_active(&self) -> bool {
        self.reason.is_some()
    }

    /// Poll for recovery: checks whether the playlist at `slides_dir` is now valid.
    ///
    /// Returns `true` if recovery has been detected and the info slide should be dismissed.
    /// On recovery, [`Self::clear`] is called automatically.
    pub fn poll_recovery(&mut self, slides_dir: &str) -> bool {
        let Some(reason) = &self.reason else {
            return false;
        };

        // Alert mode doesn't auto-recover via file polling.
        if matches!(reason, InfoReason::Alert { .. }) {
            return false;
        }

        let playlist_path = Path::new(slides_dir).join("playlist.json");

        if !playlist_path.exists() {
            // Still missing — not recovered.
            return false;
        }

        // File exists now — try to parse it.
        match std::fs::read(&playlist_path) {
            Ok(bytes) => match crate::schedule::parse_playlist(&bytes) {
                Ok(playlist) => {
                    let has_enabled = playlist.slides.iter().any(|e| e.is_enabled());
                    if has_enabled {
                        self.clear();
                        return true;
                    }
                    // Still empty — update reason to reflect current state.
                    if let Some(reason) = &mut self.reason {
                        if let InfoReason::MissingPlaylist { management_url, .. }
                        | InfoReason::InvalidPlaylist { management_url, .. } = reason
                        {
                            let url = management_url.clone();
                            *reason = InfoReason::EmptyPlaylist { management_url: url };
                        }
                    }
                    false
                }
                Err(error) => {
                    // Still invalid — update the reason with the latest error.
                    if let Some(reason) = &mut self.reason {
                        if let InfoReason::MissingPlaylist { management_url, .. }
                        | InfoReason::EmptyPlaylist { management_url, .. } = reason
                        {
                            let url = management_url.clone();
                            let clean_error = error
                                .strip_prefix("invalid playlist JSON: ")
                                .unwrap_or(&error);
                            *reason = InfoReason::InvalidPlaylist {
                                error: clean_error.to_string(),
                                management_url: url,
                            };
                        }
                    }
                    false
                }
            },
            Err(_) => false,
        }
    }
}

/// Build an [`InfoReason`] for a missing playlist.
pub fn missing_playlist_info(slides_dir: &str, management_url: &str) -> InfoReason {
    InfoReason::MissingPlaylist {
        slides_dir: slides_dir.to_string(),
        management_url: management_url.to_string(),
    }
}

/// Build an [`InfoReason`] for an invalid playlist.
pub fn invalid_playlist_info(error: &str, management_url: &str) -> InfoReason {
    InfoReason::InvalidPlaylist {
        error: error.to_string(),
        management_url: management_url.to_string(),
    }
}

/// Build an [`InfoReason`] for an empty playlist.
pub fn empty_playlist_info(management_url: &str) -> InfoReason {
    InfoReason::EmptyPlaylist {
        management_url: management_url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_state_starts_inactive() {
        let state = InfoState::new();
        assert!(!state.is_active());
    }

    #[test]
    fn show_sets_reason() {
        let mut state = InfoState::new();
        state.show(missing_playlist_info("/slides", "http://localhost:8080"));
        assert!(state.is_active());
    }

    #[test]
    fn clear_resets() {
        let mut state = InfoState::new();
        state.show(missing_playlist_info("/slides", "http://localhost:8080"));
        state.clear();
        assert!(!state.is_active());
    }

    #[test]
    fn alert_does_not_auto_recover() {
        let mut state = InfoState::new();
        state.show(InfoReason::Alert {
            title: "Network error".into(),
            lines: vec!["API unavailable".into()],
        });
        assert!(!state.poll_recovery("/tmp/nonexistent"));
        assert!(state.is_active());
    }

    #[test]
    fn missing_playlist_primary_message() {
        let reason = missing_playlist_info("/my/slides", "http://0.0.0.0:8080");
        assert_eq!(reason.primary_message(), "No playlist.json found in '/my/slides'");
    }
}
