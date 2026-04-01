//! Core engine state machine.
//!
//! This module contains the main engine loop that manages:
//! - Slide scheduling
//! - Transition resolution
//! - Frame timing
//! - Render command generation

use std::time::Duration;

use crate::lifecycle::SlideState;
use crate::schedule::Playlist;
use crate::transition::{ActiveTransition, TransitionKind, TransitionState, resolve_transition};
use crate::types::{EngineInput, EngineOutput, EngineState, InputEvent, LogLevel, RenderCommand};
use crate::Host;

/// Default slide duration in seconds.
pub const DEFAULT_SLIDE_DURATION: f32 = 7.0;
/// Minimum slide duration in seconds.
pub const MIN_SLIDE_DURATION: f32 = 1.0;
/// Maximum slide duration in seconds.
pub const MAX_SLIDE_DURATION: f32 = 300.0;

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Default duration for slides without explicit duration.
    pub default_duration_secs: f32,
    /// Default transition kind.
    pub default_transition: TransitionKind,
    /// Transition duration.
    pub transition_duration: Duration,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            default_duration_secs: DEFAULT_SLIDE_DURATION,
            default_transition: TransitionKind::Crossfade,
            transition_duration: Duration::from_millis(1000),
        }
    }
}

/// Slide entry in the schedule.
#[derive(Debug, Clone)]
pub struct SlideEntry {
    /// Path to the slide.
    pub path: String,
    /// Duration in seconds.
    pub duration_secs: f32,
    /// Transition-in kind.
    pub transition_in: Option<TransitionKind>,
    /// Transition-out kind.
    pub transition_out: Option<TransitionKind>,
    /// Current state.
    pub state: SlideState,
    /// Elapsed time in this slide.
    pub elapsed_secs: f32,
}

impl SlideEntry {
    /// Creates a new slide entry.
    pub fn new(path: String, duration_secs: f32) -> Self {
        Self {
            path,
            duration_secs,
            transition_in: None,
            transition_out: None,
            state: SlideState::Unloaded,
            elapsed_secs: 0.0,
        }
    }

    /// Returns true if a transition should start.
    pub fn should_transition(&self) -> bool {
        self.elapsed_secs >= self.duration_secs
    }
}

/// The main engine state machine.
///
/// The engine manages the slide schedule, transitions, and frame timing.
/// It generates platform-agnostic render commands for the host to execute.
pub struct Engine {
    /// Engine configuration.
    config: EngineConfig,
    /// Slide schedule.
    schedule: Vec<SlideEntry>,
    /// Current schedule index.
    current_index: usize,
    /// Transition state.
    transition: TransitionState,
    /// Time of last slide switch.
    last_switch_time_secs: f32,
    /// Total engine running time.
    total_time_secs: f32,
    /// Frame timing history.
    frame_times: Vec<f32>,
    /// Current FPS.
    fps: f32,
    /// Whether the engine is initialized.
    is_initialized: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Creates a new engine with default configuration.
    pub fn new() -> Self {
        Self {
            config: EngineConfig::default(),
            schedule: Vec::new(),
            current_index: 0,
            transition: TransitionState::Idle,
            last_switch_time_secs: 0.0,
            total_time_secs: 0.0,
            frame_times: Vec::with_capacity(30),
            fps: 0.0,
            is_initialized: false,
        }
    }

    /// Creates a new engine with custom configuration.
    pub fn with_config(config: EngineConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Sets the slide schedule.
    ///
    /// # Arguments
    /// * `slides` - Vector of slide paths
    pub fn set_schedule(&mut self, slides: Vec<String>) {
        self.schedule = slides
            .into_iter()
            .map(|path| SlideEntry::new(path, self.config.default_duration_secs))
            .collect();
        self.current_index = 0;
        if !self.schedule.is_empty() {
            self.schedule[0].state = SlideState::Loaded;
        }
    }

    /// Sets the schedule from a playlist.
    ///
    /// # Arguments
    /// * `playlist` - The playlist to use
    /// * `base_path` - Base path to prepend to slide paths
    pub fn set_schedule_from_playlist(&mut self, playlist: &Playlist, base_path: &str) {
        let slides = crate::schedule::build_schedule_from_playlist(playlist, base_path);
        
        // Apply playlist defaults
        self.schedule = slides
            .into_iter()
            .map(|path| {
                // Find the entry in the playlist
                let entry = playlist.slides.iter().find(|e| {
                    let full_path = if base_path.ends_with('/') {
                        format!("{}{}", base_path, e.path)
                    } else {
                        format!("{}/{}", base_path, e.path)
                    };
                    full_path == path
                });

                let duration = entry
                    .map(|e| crate::schedule::resolve_duration(e, &playlist.defaults, self.config.default_duration_secs))
                    .unwrap_or(self.config.default_duration_secs);

                let mut slide = SlideEntry::new(path, duration);

                if let Some(entry) = entry {
                    slide.transition_in = entry.transition_in.as_deref().map(|s| {
                        crate::slide::manifest::parse_transition_kind(s)
                    });
                    slide.transition_out = entry.transition_out.as_deref().map(|s| {
                        crate::slide::manifest::parse_transition_kind(s)
                    });
                }

                slide
            })
            .collect();

        self.current_index = 0;
        if !self.schedule.is_empty() {
            self.schedule[0].state = SlideState::Loaded;
        }
    }

    /// Returns the current engine state.
    pub fn state(&self) -> EngineState {
        let current = self.schedule.get(self.current_index);
        EngineState {
            schedule_index: self.current_index,
            total_slides: self.schedule.len(),
            elapsed_secs: current.map(|s| s.elapsed_secs).unwrap_or(0.0),
            fps: self.fps,
            is_transitioning: self.transition.is_active(),
            is_ready: self.is_initialized && !self.schedule.is_empty(),
        }
    }

    /// Returns the current slide index.
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Returns the total number of slides.
    pub fn total_slides(&self) -> usize {
        self.schedule.len()
    }

    /// Returns the current slide path.
    pub fn current_slide_path(&self) -> Option<&str> {
        self.schedule.get(self.current_index).map(|s| s.path.as_str())
    }

    /// Main engine update loop.
    ///
    /// # Arguments
    /// * `host` - The host implementation
    /// * `input` - Input for this frame
    ///
    /// # Returns
    /// Engine output with render commands and state
    pub fn update(&mut self, host: &mut impl Host, input: EngineInput) -> EngineOutput {
        // Update timing
        self.total_time_secs += input.dt;
        self.update_fps(input.dt);

        // Process input events
        for event in &input.events {
            self.process_event(host, event);
        }

        // Update current slide
        if let Some(current) = self.schedule.get_mut(self.current_index) {
            if current.state.is_active() {
                current.elapsed_secs += input.dt;
            }
        }

        // Check for transition
        self.check_transition();

        // Update transition if active
        if let Some(transition) = self.transition.as_active() {
            if transition.is_complete(self.total_time_secs) {
                self.complete_transition();
            }
        }

        // Generate render commands
        let commands = self.generate_render_commands();

        // Build output
        EngineOutput {
            commands,
            state: self.state(),
            requests: Vec::new(),
        }
    }

    /// Updates the FPS calculation.
    fn update_fps(&mut self, dt: f32) {
        if dt > 0.0 {
            self.frame_times.push(1.0 / dt);
            if self.frame_times.len() > 30 {
                self.frame_times.remove(0);
            }
            self.fps = self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32;
        }
    }

    /// Processes an input event.
    fn process_event(&mut self, host: &mut impl Host, event: &InputEvent) {
        match event {
            InputEvent::Resized { width, height } => {
                // Host handles resize, kernel just notes it
                host.log(LogLevel::Debug, &format!("Viewport resized to {}x{}", width, height));
            }
            InputEvent::DataReady { key, data } => {
                // Data request fulfilled by host
                host.log(LogLevel::Debug, &format!("Data ready for key: {}", key));
            }
            InputEvent::UserInput { kind } => {
                // Forward to slide (handled by host)
            }
        }
    }

    /// Checks if a transition should start.
    fn check_transition(&mut self) {
        if self.transition.is_active() {
            return;
        }

        if self.schedule.is_empty() {
            return;
        }

        let current = &self.schedule[self.current_index];
        if current.should_transition() {
            self.start_transition();
        }
    }

    /// Starts a transition to the next slide.
    fn start_transition(&mut self) {
        if self.schedule.len() <= 1 {
            return;
        }

        let current_idx = self.current_index;
        let next_idx = (self.current_index + 1) % self.schedule.len();

        // Resolve transition kind
        let outgoing = self.schedule.get(current_idx);
        let incoming = self.schedule.get(next_idx);

        let kind = resolve_transition(
            outgoing.and_then(|s| s.transition_out),
            incoming.and_then(|s| s.transition_in),
            Some(self.config.default_transition),
        );

        // Handle cut transition (instant)
        if kind == TransitionKind::Cut {
            self.complete_transition();
            return;
        }

        // Start blending transition
        self.transition = TransitionState::Blending(ActiveTransition::new(
            kind,
            current_idx,
            self.config.transition_duration,
            self.total_time_secs,
        ));

        // Update slide states
        if let Some(current) = self.schedule.get_mut(current_idx) {
            current.state = SlideState::Parked;
        }
        if let Some(next) = self.schedule.get_mut(next_idx) {
            next.state = SlideState::Loaded;
            next.elapsed_secs = 0.0;
        }
    }

    /// Completes the current transition.
    fn complete_transition(&mut self) {
        if let TransitionState::Blending(transition) = &self.transition {
            let next_idx = (transition.outgoing_idx + 1) % self.schedule.len();

            // Update slide states
            if let Some(old) = self.schedule.get_mut(transition.outgoing_idx) {
                old.state = SlideState::Unloaded;
                old.elapsed_secs = 0.0;
            }
            if let Some(new) = self.schedule.get_mut(next_idx) {
                new.state = SlideState::Active;
            }

            self.current_index = next_idx;
            self.last_switch_time_secs = self.total_time_secs;
        }

        self.transition = TransitionState::Idle;
    }

    /// Generates render commands for the current frame.
    fn generate_render_commands(&self) -> Vec<RenderCommand> {
        let mut commands = Vec::new();

        // Begin frame
        commands.push(RenderCommand::BeginFrame);

        // Clear
        commands.push(RenderCommand::Clear {
            color: Some([0.0, 0.0, 0.0, 1.0]),
            depth: Some(1.0),
        });

        // Render current slide
        if let Some(current) = self.schedule.get(self.current_index) {
            if current.state.can_render() {
                // Bind pipeline and render
                commands.push(RenderCommand::BindPipeline {
                    kind: crate::types::PipelineKind::Opaque,
                });
                // Additional render commands would go here
                // (actual drawing is handled by host based on slide state)
            }
        }

        // End frame
        commands.push(RenderCommand::EndFrame);

        commands
    }

    /// Initializes the engine.
    pub fn init(&mut self, host: &mut impl Host) {
        host.log(LogLevel::Info, "VZGLYD kernel initialized");
        self.is_initialized = true;
    }

    /// Shuts down the engine.
    pub fn shutdown(&mut self, host: &mut impl Host) {
        host.log(LogLevel::Info, "VZGLYD kernel shutting down");
        self.is_initialized = false;
        self.schedule.clear();
        self.transition = TransitionState::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LogLevel;

    struct TestHost;

    impl Host for TestHost {
        fn request_data(&mut self, key: &str) -> Option<Vec<u8>> {
            None
        }

        fn submit_render_commands(&mut self, cmds: &[RenderCommand]) {}

        fn log(&mut self, level: LogLevel, msg: &str) {}

        fn now(&self) -> f32 {
            0.0
        }
    }

    #[test]
    fn engine_initial_state() {
        let engine = Engine::new();
        assert_eq!(engine.current_index(), 0);
        assert_eq!(engine.total_slides(), 0);
        assert!(!engine.state().is_ready);
    }

    #[test]
    fn engine_sets_schedule() {
        let mut engine = Engine::new();
        engine.set_schedule(vec!["a.vzglyd".into(), "b.vzglyd".into()]);

        assert_eq!(engine.total_slides(), 2);
        assert_eq!(engine.current_slide_path(), Some("a.vzglyd"));
    }

    #[test]
    fn engine_advances_time() {
        let mut engine = Engine::new();
        engine.set_schedule(vec!["a.vzglyd".into()]);
        engine.schedule[0].state = SlideState::Active;

        let mut host = TestHost;
        let output = engine.update(
            &mut host,
            EngineInput {
                dt: 0.1,
                events: vec![],
            },
        );

        assert!(output.state.elapsed_secs >= 0.1);
    }

    #[test]
    fn transition_resolves_kind() {
        let kind = resolve_transition(
            Some(TransitionKind::Cut),
            Some(TransitionKind::Crossfade),
            Some(TransitionKind::Dissolve),
        );
        assert_eq!(kind, TransitionKind::Cut);
    }
}
