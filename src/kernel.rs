//! Core engine state machine.
//!
//! This module contains the main engine loop that manages:
//! - Slide scheduling
//! - Transition resolution
//! - Frame timing
//! - Render command generation

use std::time::Duration;

use crate::Host;
use crate::info::InfoState;
use crate::lifecycle::SlideState;
use crate::schedule::{Playlist, ResolvedSlideEntry, ScreensaverConfig, resolve_schedule_from_playlist};
use crate::transition::{ActiveTransition, TransitionKind, TransitionState, resolve_transition};
use crate::types::{EngineInput, EngineOutput, EngineState, InputEvent, LogLevel, RenderCommand};

/// Frame rendering state returned by [`Engine::frame_state`].
///
/// Tells the host which slide(s) to render this frame and how to composite them.
/// During a transition, both `current_slide_idx` (outgoing) and `next_slide_idx`
/// (incoming) must be rendered and composited by the host.
#[derive(Debug, Clone)]
pub struct FrameRenderState {
    /// Index of the currently active slide, or the outgoing slide during a transition.
    pub current_slide_idx: usize,
    /// Index of the incoming slide during a transition. `None` when idle.
    pub next_slide_idx: Option<usize>,
    /// Smoothstepped transition progress from 0.0 (start) to 1.0 (complete).
    /// 0.0 when not transitioning.
    pub transition_progress: f32,
    /// The kind of transition currently active. `None` when idle.
    pub transition_kind: Option<TransitionKind>,
    /// Elapsed time on the current slide in seconds.
    pub elapsed_secs: f32,
    /// Total number of slides in the schedule.
    pub total_slides: usize,
    /// Present when the screensaver is active. Hosts should suppress the normal
    /// HUD border and render the intermission scene instead.
    pub screensaver: Option<ScreensaverFrameState>,
    /// Present when the information slide should be shown instead of normal content.
    /// The host should render the info slide with the provided reason's message.
    pub info: Option<crate::info::InfoReason>,
}

/// State passed to hosts when the screensaver is active.
#[derive(Debug, Clone)]
pub struct ScreensaverFrameState {
    /// Seconds remaining until the playlist resumes.
    pub remaining_secs: f32,
    /// Total screensaver duration (for progress display).
    pub total_secs: f32,
    /// Accumulated screensaver time in seconds; used by the geometry builder
    /// to compute the slow text-drift offset.
    pub elapsed_secs: f32,
}

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
            transition_duration: Duration::from_millis(600),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct PlaylistOverrides {
    duration_secs: Option<f32>,
    transition_in: Option<TransitionKind>,
    transition_out: Option<TransitionKind>,
}

/// Manifest-derived timing metadata for a loaded slide.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SlideManifestMetadata {
    /// Optional display duration from the slide manifest.
    pub duration_secs: Option<f32>,
    /// Optional transition-in from the slide manifest.
    pub transition_in: Option<TransitionKind>,
    /// Optional transition-out from the slide manifest.
    pub transition_out: Option<TransitionKind>,
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
    /// Optional JSON parameters forwarded to `vzglyd_configure`.
    pub params: Option<serde_json::Value>,
    /// Current state.
    pub state: SlideState,
    /// Elapsed time in this slide.
    pub elapsed_secs: f32,
    playlist_overrides: PlaylistOverrides,
}

impl SlideEntry {
    /// Creates a new slide entry.
    pub fn new(path: String, duration_secs: f32) -> Self {
        Self {
            path,
            duration_secs,
            transition_in: None,
            transition_out: None,
            params: None,
            state: SlideState::Unloaded,
            elapsed_secs: 0.0,
            playlist_overrides: PlaylistOverrides::default(),
        }
    }

    fn from_resolved(entry: ResolvedSlideEntry) -> Self {
        Self {
            path: entry.path,
            duration_secs: entry.duration_secs,
            transition_in: entry.transition_in,
            transition_out: entry.transition_out,
            params: entry.params,
            state: SlideState::Unloaded,
            elapsed_secs: 0.0,
            playlist_overrides: PlaylistOverrides {
                duration_secs: Some(entry.duration_secs),
                transition_in: entry.transition_in,
                transition_out: entry.transition_out,
            },
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
    /// Optional screensaver configuration extracted from the playlist.
    screensaver_config: Option<ScreensaverConfig>,
    /// Accumulated display time since the last screensaver reset (seconds).
    display_elapsed_secs: f32,
    /// Elapsed time inside the current screensaver run (seconds).
    screensaver_elapsed_secs: f32,
    /// Whether the screensaver is currently active.
    screensaver_active: bool,
    /// Information slide state — when active, the info slide is shown instead of normal content.
    info_state: InfoState,
    /// Slides directory — used for info slide recovery polling.
    slides_dir: Option<String>,
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
            screensaver_config: None,
            display_elapsed_secs: 0.0,
            screensaver_elapsed_secs: 0.0,
            screensaver_active: false,
            info_state: InfoState::new(),
            slides_dir: None,
        }
    }

    /// Configures the screensaver independently of a playlist load.
    ///
    /// `timeout_secs` is how long the display runs before activating the screensaver.
    /// `duration_secs` is how long the screensaver runs before the playlist resumes.
    /// Pass `None` to disable the screensaver.
    pub fn set_screensaver_config(&mut self, config: Option<ScreensaverConfig>) {
        self.screensaver_config = config;
        self.display_elapsed_secs = 0.0;
        self.screensaver_elapsed_secs = 0.0;
        self.screensaver_active = false;
    }

    /// Returns `true` if the screensaver is currently active.
    pub fn is_screensaver_active(&self) -> bool {
        self.screensaver_active
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
        self.mark_initial_slide_active();
    }

    /// Sets the slide schedule from pre-resolved entries.
    ///
    /// This is the canonical entrypoint for hosts that already resolved
    /// playlist metadata and want the kernel to own the resulting schedule.
    pub fn set_resolved_schedule(&mut self, slides: Vec<ResolvedSlideEntry>) {
        self.schedule = slides.into_iter().map(SlideEntry::from_resolved).collect();
        self.mark_initial_slide_active();
    }

    fn mark_initial_slide_active(&mut self) {
        self.current_index = 0;
        if !self.schedule.is_empty() {
            self.schedule[0].state = SlideState::Active;
        }
    }

    /// Sets the schedule from a playlist.
    ///
    /// Also extracts screensaver configuration from `playlist.defaults.screensaver` when present.
    ///
    /// # Arguments
    /// * `playlist` - The playlist to use
    /// * `base_path` - Base path to prepend to slide paths
    pub fn set_schedule_from_playlist(&mut self, playlist: &Playlist, base_path: &str) {
        let slides =
            resolve_schedule_from_playlist(playlist, base_path, self.config.default_duration_secs);
        self.set_resolved_schedule(slides);
        self.screensaver_config = playlist.defaults.screensaver.clone();
        self.display_elapsed_secs = 0.0;
        self.screensaver_elapsed_secs = 0.0;
        self.screensaver_active = false;
    }

    // ── Information slide ────────────────────────────────────────────────

    /// Set the slides directory for recovery polling.
    ///
    /// Call this when using `--slides-dir` so the kernel can poll for
    /// playlist.json appearance / validity while the info slide is shown.
    pub fn set_slides_dir(&mut self, dir: &str) {
        self.slides_dir = Some(dir.to_string());
    }

    /// Show the information slide with the given reason.
    ///
    /// The host should render the info slide instead of normal content until
    /// [`Self::poll_info_recovery`] returns `true`.
    pub fn show_info_slide(&mut self, reason: crate::info::InfoReason) {
        self.info_state.show(reason);
    }

    /// Clear the information slide — normal playlist operation resumes.
    pub fn clear_info_slide(&mut self) {
        self.info_state.clear();
    }

    /// Poll for recovery when the info slide is active.
    ///
    /// Returns `true` if the underlying issue (e.g. missing playlist.json)
    /// has been resolved and normal operation can resume.
    pub fn poll_info_recovery(&mut self) -> bool {
        let Some(slides_dir) = &self.slides_dir else {
            return false;
        };
        self.info_state.poll_recovery(slides_dir)
    }

    /// Returns the current info reason, if the info slide is active.
    pub fn info_reason(&self) -> Option<&crate::info::InfoReason> {
        self.info_state.reason.as_ref()
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
        self.schedule
            .get(self.current_index)
            .map(|s| s.path.as_str())
    }

    /// Returns the current schedule entries.
    pub fn schedule_entries(&self) -> &[SlideEntry] {
        &self.schedule
    }

    /// Returns a single schedule entry by index.
    pub fn slide_entry(&self, index: usize) -> Option<&SlideEntry> {
        self.schedule.get(index)
    }

    /// Applies manifest timing metadata to a loaded slide.
    ///
    /// Playlist overrides remain authoritative when present.
    pub fn apply_manifest_metadata(&mut self, index: usize, manifest: SlideManifestMetadata) {
        let Some(slide) = self.schedule.get_mut(index) else {
            return;
        };

        slide.duration_secs = slide
            .playlist_overrides
            .duration_secs
            .or(manifest.duration_secs)
            .unwrap_or(self.config.default_duration_secs);
        slide.transition_in = slide
            .playlist_overrides
            .transition_in
            .or(manifest.transition_in);
        slide.transition_out = slide
            .playlist_overrides
            .transition_out
            .or(manifest.transition_out);
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

        // Advance screensaver state machine.
        self.update_screensaver(input.dt);

        // Only advance slide timing when the screensaver is not active.
        if !self.screensaver_active {
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

    /// Advances the screensaver state machine by `dt` seconds.
    fn update_screensaver(&mut self, dt: f32) {
        let Some(config) = &self.screensaver_config else {
            return;
        };
        let timeout = config.timeout_seconds as f32;
        let duration = config.duration_seconds as f32;

        if self.screensaver_active {
            self.screensaver_elapsed_secs += dt;
            if self.screensaver_elapsed_secs >= duration {
                // Resume the playlist.
                self.screensaver_active = false;
                self.screensaver_elapsed_secs = 0.0;
                self.display_elapsed_secs = 0.0;
            }
        } else {
            self.display_elapsed_secs += dt;
            if self.display_elapsed_secs >= timeout {
                self.screensaver_active = true;
                self.screensaver_elapsed_secs = 0.0;
            }
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
                host.log(
                    LogLevel::Debug,
                    &format!("Viewport resized to {}x{}", width, height),
                );
            }
            InputEvent::DataReady { key, data: _ } => {
                // Data request fulfilled by host
                host.log(LogLevel::Debug, &format!("Data ready for key: {}", key));
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

    /// Returns the frame rendering state for the current frame.
    ///
    /// Use this to determine which slide(s) to render and how to composite them.
    /// During a transition, `current_slide_idx` is the outgoing slide and
    /// `next_slide_idx` is the incoming slide.
    ///
    /// When `screensaver` is `Some`, hosts should suppress the normal HUD border
    /// and slide content, rendering the intermission scene instead.
    pub fn frame_state(&self) -> FrameRenderState {
        let current = self.schedule.get(self.current_index);
        let elapsed_secs = current.map(|s| s.elapsed_secs).unwrap_or(0.0);

        let screensaver = if self.screensaver_active {
            self.screensaver_config.as_ref().map(|cfg| ScreensaverFrameState {
                remaining_secs: (cfg.duration_seconds as f32 - self.screensaver_elapsed_secs).max(0.0),
                total_secs: cfg.duration_seconds as f32,
                elapsed_secs: self.screensaver_elapsed_secs,
            })
        } else {
            None
        };

        match &self.transition {
            TransitionState::Idle => FrameRenderState {
                current_slide_idx: self.current_index,
                next_slide_idx: None,
                transition_progress: 0.0,
                transition_kind: None,
                elapsed_secs,
                total_slides: self.schedule.len(),
                screensaver,
                info: self.info_state.reason.clone(),
            },
            TransitionState::Blending(active) => {
                let next_idx = if self.schedule.len() > 1 {
                    (active.outgoing_idx + 1) % self.schedule.len()
                } else {
                    active.outgoing_idx
                };
                FrameRenderState {
                    current_slide_idx: active.outgoing_idx,
                    next_slide_idx: Some(next_idx),
                    transition_progress: active.smooth_progress(self.total_time_secs),
                    transition_kind: Some(active.kind),
                    elapsed_secs,
                    total_slides: self.schedule.len(),
                    screensaver,
                    info: self.info_state.reason.clone(),
                }
            }
        }
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
        fn request_data(&mut self, _key: &str) -> Option<Vec<u8>> {
            None
        }

        fn submit_render_commands(&mut self, _cmds: &[RenderCommand]) {}

        fn log(&mut self, _level: LogLevel, _msg: &str) {}

        fn now(&self) -> f32 {
            0.0
        }
    }

    #[test]
    fn screensaver_activates_after_timeout() {
        let mut engine = Engine::new();
        engine.set_schedule(vec!["a.vzglyd".into(), "b.vzglyd".into()]);
        engine.schedule[0].state = SlideState::Active;
        engine.set_screensaver_config(Some(ScreensaverConfig {
            timeout_seconds: 5,
            duration_seconds: 10,
        }));

        let mut host = TestHost;

        // Advance just under the timeout — screensaver should not be active.
        engine.update(&mut host, EngineInput { dt: 4.9, events: vec![] });
        assert!(!engine.is_screensaver_active());
        assert!(engine.frame_state().screensaver.is_none());

        // Push past the timeout.
        engine.update(&mut host, EngineInput { dt: 0.2, events: vec![] });
        assert!(engine.is_screensaver_active());
        let ss = engine.frame_state().screensaver.expect("screensaver state");
        assert!(ss.remaining_secs <= 10.0);
        assert!(ss.elapsed_secs >= 0.0);
    }

    #[test]
    fn screensaver_resumes_after_duration() {
        let mut engine = Engine::new();
        engine.set_schedule(vec!["a.vzglyd".into()]);
        engine.schedule[0].state = SlideState::Active;
        engine.set_screensaver_config(Some(ScreensaverConfig {
            timeout_seconds: 1,
            duration_seconds: 3,
        }));

        let mut host = TestHost;

        // Trigger screensaver.
        engine.update(&mut host, EngineInput { dt: 1.1, events: vec![] });
        assert!(engine.is_screensaver_active());

        // Advance through the screensaver duration.
        engine.update(&mut host, EngineInput { dt: 3.1, events: vec![] });
        assert!(!engine.is_screensaver_active());
        assert!(engine.frame_state().screensaver.is_none());
    }

    #[test]
    fn screensaver_pauses_slide_advancement() {
        let mut engine = Engine::new();
        engine.set_schedule(vec!["a.vzglyd".into()]);
        engine.schedule[0].state = SlideState::Active;
        engine.set_screensaver_config(Some(ScreensaverConfig {
            timeout_seconds: 2,
            duration_seconds: 5,
        }));

        let mut host = TestHost;

        // Advance 2 seconds normally.
        engine.update(&mut host, EngineInput { dt: 2.0, events: vec![] });
        let elapsed_before = engine.slide_entry(0).unwrap().elapsed_secs;

        // Trigger screensaver.
        engine.update(&mut host, EngineInput { dt: 0.1, events: vec![] });
        assert!(engine.is_screensaver_active());

        // During screensaver, slide elapsed should not advance.
        engine.update(&mut host, EngineInput { dt: 1.0, events: vec![] });
        let elapsed_during = engine.slide_entry(0).unwrap().elapsed_secs;
        assert!(
            (elapsed_during - elapsed_before).abs() < 0.01,
            "slide elapsed advanced during screensaver: before={elapsed_before} during={elapsed_during}"
        );
    }

    #[test]
    fn screensaver_disabled_when_no_config() {
        let mut engine = Engine::new();
        engine.set_schedule(vec!["a.vzglyd".into()]);
        engine.schedule[0].state = SlideState::Active;

        let mut host = TestHost;
        // Advance a lot — no screensaver config, should never activate.
        engine.update(&mut host, EngineInput { dt: 9999.0, events: vec![] });
        assert!(!engine.is_screensaver_active());
        assert!(engine.frame_state().screensaver.is_none());
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
        assert_eq!(
            engine.slide_entry(0).map(|entry| entry.state),
            Some(SlideState::Active)
        );
    }

    #[test]
    fn default_transition_duration_is_six_hundred_ms() {
        assert_eq!(
            EngineConfig::default().transition_duration,
            Duration::from_millis(600)
        );
    }

    #[test]
    fn manifest_metadata_fills_missing_playlist_values_only() {
        let mut engine = Engine::new();
        engine.set_resolved_schedule(vec![ResolvedSlideEntry {
            path: "clock.vzglyd".into(),
            duration_secs: 20.0,
            transition_in: Some(TransitionKind::Crossfade),
            transition_out: None,
            params: Some(serde_json::json!({"mode":"demo"})),
        }]);

        engine.apply_manifest_metadata(
            0,
            SlideManifestMetadata {
                duration_secs: Some(99.0),
                transition_in: Some(TransitionKind::Dissolve),
                transition_out: Some(TransitionKind::Cut),
            },
        );

        let entry = engine.slide_entry(0).expect("slide entry");
        assert_eq!(entry.duration_secs, 20.0);
        assert_eq!(entry.transition_in, Some(TransitionKind::Crossfade));
        assert_eq!(entry.transition_out, Some(TransitionKind::Cut));
        assert_eq!(entry.params, Some(serde_json::json!({"mode":"demo"})));
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
