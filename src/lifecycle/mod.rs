//! Slide lifecycle management.
//!
//! This module handles the slide lifecycle states:
//! - Unloaded: Slide not yet loaded
//! - Loaded: Slide spec loaded, resources allocated
//! - Active: Slide is currently rendering
//! - Parked: Slide is loaded but not active (for transitions)
//! - Unloading: Slide is being cleaned up

use serde::{Deserialize, Serialize};

/// Slide lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SlideState {
    /// Slide is not loaded.
    #[default]
    Unloaded,
    /// Slide spec is loaded, GPU resources allocated.
    Loaded,
    /// Slide is actively rendering.
    Active,
    /// Slide is loaded but not rendering (waiting for transition).
    Parked,
    /// Slide is being cleaned up.
    Unloading,
}

impl SlideState {
    /// Returns true if the slide is loaded or active.
    pub fn is_loaded(self) -> bool {
        matches!(self, Self::Loaded | Self::Active | Self::Parked)
    }

    /// Returns true if the slide is currently active.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns true if the slide can be rendered.
    pub fn can_render(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Slide lifecycle events.
#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleEvent {
    /// Slide initialization requested.
    Init,
    /// Slide update with delta time.
    Update {
        /// Delta time for the update.
        dt: f32,
    },
    /// Slide should be parked.
    Park,
    /// Slide should be unloaded.
    Unload,
}

/// Result of a slide update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateResult {
    /// Slide continues running.
    Continue,
    /// Slide has completed and should stop.
    Stop,
    /// Slide encountered an error.
    Error,
}

impl UpdateResult {
    /// Returns true if the slide should continue.
    pub fn should_continue(self) -> bool {
        matches!(self, Self::Continue)
    }

    /// Returns true if the slide should stop.
    pub fn should_stop(self) -> bool {
        matches!(self, Self::Stop)
    }
}

/// Converts raw ABI update return code to UpdateResult.
///
/// # Arguments
/// * `code` - Return code from vzglyd_update
///
/// # Returns
/// UpdateResult based on the code:
/// - 0: Continue
/// - 1: Stop
/// - Other: Error
pub fn abi_code_to_result(code: i32) -> UpdateResult {
    match code {
        0 => UpdateResult::Continue,
        1 => UpdateResult::Stop,
        _ => UpdateResult::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_state_transitions() {
        let mut state = SlideState::Unloaded;
        assert!(!state.is_loaded());
        assert!(!state.is_active());

        state = SlideState::Loaded;
        assert!(state.is_loaded());
        assert!(!state.is_active());

        state = SlideState::Active;
        assert!(state.is_loaded());
        assert!(state.is_active());
        assert!(state.can_render());

        state = SlideState::Parked;
        assert!(state.is_loaded());
        assert!(!state.is_active());
        assert!(!state.can_render());
    }

    #[test]
    fn update_result_codes() {
        assert_eq!(abi_code_to_result(0), UpdateResult::Continue);
        assert_eq!(abi_code_to_result(1), UpdateResult::Stop);
        assert_eq!(abi_code_to_result(-1), UpdateResult::Error);
        assert_eq!(abi_code_to_result(999), UpdateResult::Error);
    }

    #[test]
    fn update_result_predicates() {
        assert!(UpdateResult::Continue.should_continue());
        assert!(!UpdateResult::Continue.should_stop());

        assert!(!UpdateResult::Stop.should_continue());
        assert!(UpdateResult::Stop.should_stop());
    }
}
