//! Transition types and state machine.
//!
//! This module defines the available transition kinds and the transition state machine.
//! The actual rendering of transitions is handled by the host through render commands.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Transition kinds supported by the engine.
///
/// Each transition kind has a unique shader tag used by the host to select
/// the appropriate transition shader.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    /// Crossfade (opacity blend between outgoing and incoming)
    Crossfade = 0,
    /// Wipe from right to left
    WipeLeft = 1,
    /// Wipe from top to bottom
    WipeDown = 2,
    /// Dissolve (random pixel reveal)
    Dissolve = 3,
    /// Cut (instant transition, no compositor needed)
    Cut = 4,
}

impl TransitionKind {
    /// Returns true if this transition requires compositing.
    ///
    /// [`TransitionKind::Cut`] returns false as it is an instant switch.
    pub const fn uses_compositor(self) -> bool {
        !matches!(self, Self::Cut)
    }

    /// Returns the shader tag for this transition kind.
    ///
    /// The host uses this tag to select the appropriate transition shader.
    pub const fn shader_tag(self) -> u32 {
        match self {
            Self::Crossfade => 0,
            Self::WipeLeft => 1,
            Self::WipeDown => 2,
            Self::Dissolve => 3,
            Self::Cut => 0,
        }
    }
}

impl Default for TransitionKind {
    fn default() -> Self {
        Self::Crossfade
    }
}

/// Transition state machine.
///
/// The kernel manages transitions at a high level (kind, duration, progress)
/// while the host handles the actual rendering through render commands.
#[derive(Debug, Clone, Default)]
pub enum TransitionState {
    /// No transition active.
    #[default]
    Idle,
    /// Transition in progress.
    Blending(ActiveTransition),
}

impl TransitionState {
    /// Returns true if no transition is active.
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Returns true if a transition is currently active.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Blending(_))
    }

    /// Returns the active transition if any.
    pub fn as_active(&self) -> Option<&ActiveTransition> {
        match self {
            Self::Blending(t) => Some(t),
            Self::Idle => None,
        }
    }
}

/// An active transition in progress.
///
/// This struct tracks the transition state including timing and slide indices.
#[derive(Debug, Clone)]
pub struct ActiveTransition {
    /// The kind of transition being performed.
    pub kind: TransitionKind,
    /// Index of the outgoing slide.
    pub outgoing_idx: usize,
    /// Duration of the transition.
    pub duration: Duration,
    /// Start time in seconds (from engine start).
    pub start_time_secs: f32,
}

impl ActiveTransition {
    /// Creates a new active transition.
    ///
    /// # Arguments
    /// * `kind` - The transition kind
    /// * `outgoing_idx` - Index of the slide being transitioned from
    /// * `duration` - How long the transition should take
    /// * `start_time_secs` - Engine time when the transition started
    pub fn new(
        kind: TransitionKind,
        outgoing_idx: usize,
        duration: Duration,
        start_time_secs: f32,
    ) -> Self {
        Self {
            kind,
            outgoing_idx,
            duration,
            start_time_secs,
        }
    }

    /// Returns the progress of the transition as a value between 0.0 and 1.0.
    ///
    /// # Arguments
    /// * `current_time_secs` - Current engine time in seconds
    pub fn progress(&self, current_time_secs: f32) -> f32 {
        if self.duration.is_zero() {
            1.0
        } else {
            let elapsed = (current_time_secs - self.start_time_secs).max(0.0);
            (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0)
        }
    }

    /// Returns the smoothed progress using smoothstep interpolation.
    ///
    /// # Arguments
    /// * `current_time_secs` - Current engine time in seconds
    pub fn smooth_progress(&self, current_time_secs: f32) -> f32 {
        smoothstep(self.progress(current_time_secs))
    }

    /// Returns true if the transition is complete.
    ///
    /// # Arguments
    /// * `current_time_secs` - Current engine time in seconds
    pub fn is_complete(&self, current_time_secs: f32) -> bool {
        self.progress(current_time_secs) >= 1.0
    }
}

/// Smoothstep interpolation function.
///
/// Applies the smoothstep function: t²(3 - 2t) for smooth transitions.
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Resolves the transition kind to use for a transition.
///
/// Priority order:
/// 1. Outgoing slide's transition_out
/// 2. Incoming slide's transition_in
/// 3. Manifest default
/// 4. Default (Crossfade)
///
/// # Arguments
/// * `outgoing_transition_out` - Transition out from the outgoing slide (if any)
/// * `incoming_transition_in` - Transition in from the incoming slide (if any)
/// * `manifest_default` - Default from manifest (if any)
pub fn resolve_transition(
    outgoing_transition_out: Option<TransitionKind>,
    incoming_transition_in: Option<TransitionKind>,
    manifest_default: Option<TransitionKind>,
) -> TransitionKind {
    outgoing_transition_out
        .or(incoming_transition_in)
        .or(manifest_default)
        .unwrap_or(TransitionKind::Crossfade)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_transition_tags_are_stable() {
        assert_eq!(TransitionKind::Crossfade.shader_tag(), 0);
        assert_eq!(TransitionKind::WipeLeft.shader_tag(), 1);
        assert_eq!(TransitionKind::WipeDown.shader_tag(), 2);
        assert_eq!(TransitionKind::Dissolve.shader_tag(), 3);
    }

    #[test]
    fn cut_skips_compositor_path() {
        assert!(!TransitionKind::Cut.uses_compositor());
        assert!(TransitionKind::Crossfade.uses_compositor());
        assert!(TransitionKind::WipeLeft.uses_compositor());
        assert!(TransitionKind::WipeDown.uses_compositor());
        assert!(TransitionKind::Dissolve.uses_compositor());
    }

    #[test]
    fn smoothstep_clamps_to_unit_interval() {
        assert_eq!(smoothstep(-1.0), 0.0);
        assert_eq!(smoothstep(2.0), 1.0);
    }

    #[test]
    fn smoothstep_keeps_midpoint_fixed() {
        assert!((smoothstep(0.5) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn active_transition_progress() {
        let transition = ActiveTransition::new(
            TransitionKind::Crossfade,
            0,
            Duration::from_secs(2),
            0.0,
        );

        assert_eq!(transition.progress(0.0), 0.0);
        assert_eq!(transition.progress(1.0), 0.5);
        assert_eq!(transition.progress(2.0), 1.0);
        assert_eq!(transition.progress(3.0), 1.0); // Clamped
    }

    #[test]
    fn active_transition_completion() {
        let transition = ActiveTransition::new(
            TransitionKind::Crossfade,
            0,
            Duration::from_secs(2),
            0.0,
        );

        assert!(!transition.is_complete(0.0));
        assert!(!transition.is_complete(1.0));
        assert!(transition.is_complete(2.0));
        assert!(transition.is_complete(3.0));
    }

    #[test]
    fn resolve_transition_priority() {
        // Outgoing takes priority
        let result = resolve_transition(
            Some(TransitionKind::Cut),
            Some(TransitionKind::Crossfade),
            Some(TransitionKind::Dissolve),
        );
        assert_eq!(result, TransitionKind::Cut);

        // Incoming if no outgoing
        let result = resolve_transition(
            None,
            Some(TransitionKind::WipeLeft),
            Some(TransitionKind::Dissolve),
        );
        assert_eq!(result, TransitionKind::WipeLeft);

        // Manifest default if no slide overrides
        let result = resolve_transition(None, None, Some(TransitionKind::WipeDown));
        assert_eq!(result, TransitionKind::WipeDown);

        // Crossfade as final default
        let result = resolve_transition(None, None, None);
        assert_eq!(result, TransitionKind::Crossfade);
    }
}
