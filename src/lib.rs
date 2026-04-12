//! VZGLYD Kernel - Platform-agnostic display engine core
//!
//! This crate contains the platform-agnostic core of the VZGLYD display engine.
//! It handles slide loading, scheduling, transitions, and render command generation
//! without any platform-specific dependencies.
//!
//! # Architecture
//!
//! The kernel is designed to work with any graphics backend through the [`Host`] trait:
//!
//! ```text
//! ┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
//! │  Native Host    │     │   VZGLYD Kernel  │     │   WebGPU Host   │
//! │  (wgpu/winit)   │────▶│  (platform-agnostic)│◀────│  (web)          │
//! └─────────────────┘     └──────────────────┘     └─────────────────┘
//!         │                       │                        │
//!         └───────────────────────┼────────────────────────┘
//!                                 │
//!                    ┌────────────▼────────────┐
//!                    │    Host Trait           │
//!                    │  - request_data()       │
//!                    │  - submit_render_commands()│
//!                    │  - log()                │
//!                    │  - now()                │
//!                    └─────────────────────────┘
//! ```
//!
//! # Usage
//!
//! Implement the [`Host`] trait for your platform, then use [`Engine`] to run
//! the display engine:
//!
//! ```rust,no_run
//! use vzglyd_kernel::{Engine, EngineInput, Host};
//!
//! struct MyHost {
//!     // Platform-specific state
//! }
//!
//! impl Host for MyHost {
//!     fn request_data(&mut self, key: &str) -> Option<Vec<u8>> {
//!         // Load data from filesystem, network, etc.
//!         None
//!     }
//!
//!     fn submit_render_commands(&mut self, cmds: &[vzglyd_kernel::RenderCommand]) {
//!         // Execute render commands using native graphics API
//!     }
//!
//!     fn log(&mut self, level: vzglyd_kernel::LogLevel, msg: &str) {
//!         // Log message
//!     }
//!
//!     fn now(&self) -> f32 {
//!         // Return current time in seconds
//!         0.0
//!     }
//! }
//!
//! let mut engine = Engine::new();
//! let mut host = MyHost { /* ... */ };
//!
//! // Main loop
//! loop {
//!     let input = EngineInput {
//!         dt: 0.016, // 60 FPS
//!         events: vec![],
//!     };
//!     let output = engine.update(&mut host, input);
//!     // output.commands contains render commands to execute
//! }
//! ```
//!
//! # Modules
//!
//! - [`slide`]: Slide loading, manifest validation, and SlideSpec decoding
//! - [`schedule`]: Playlist parsing and schedule management
//! - [`transition`]: Transition types and state machine
//! - [`lifecycle`]: Slide lifecycle management (init/update/park/teardown)
//! - [`shader`]: Shader validation against the VZGLYD contract
//! - [`types`]: Core types including [`Host`], [`RenderCommand`], [`EngineInput`], [`EngineOutput`]
//! - [`glb`]: GLB file loading and parsing

#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

pub mod glb;
pub mod info;
pub mod kernel;
pub mod management;
pub mod overlay;
pub mod lifecycle;
pub mod schedule;
pub mod manifest;
pub mod shader;
pub mod trace;
pub mod transition;
pub mod types;
pub mod audio;

// Re-export main types
pub use types::{
    BufferHandle, BufferUsage, DataRequest, EngineInput, EngineOutput, EngineState, Host,
    InputEvent, LogLevel, PipelineKind, RenderCommand, SamplerHandle, TextureFormat,
    TextureHandle,
};
pub use audio::{SoundDesc, SoundFormat};

// Re-export main engine types
pub use kernel::{Engine, EngineConfig, FrameRenderState, ScreensaverFrameState, SlideEntry, SlideManifestMetadata};
pub use transition::TransitionKind;

// Re-export overlay types
pub use overlay::{
    BORDER_PX, COLOR_BORDER, COLOR_CLOCK, COLOR_FOOTER_BG, COLOR_TITLE, COLOR_UPDATED, FOOTER_PX,
    GLYPH_SIZE, OverlayVertex, TEXT_MARGIN_PX, TEXT_SCALE, build_font_atlas_pixels,
    build_hud_geometry, build_hud_geometry_with_update, build_info_geometry, build_screensaver_geometry,
    normalize_text,
};

// Re-export schedule types
pub use schedule::ScreensaverConfig;

// Re-export info slide types
pub use info::{InfoReason, InfoState, empty_playlist_info, invalid_playlist_info, missing_playlist_info};

// Re-export management types
pub use management::{
    ENGINE_DEFAULT_DURATION_SECS, HydratedPlaylistEntry, SECRETS_FILENAME, SecretsStore,
    SlideLibraryEntry, SoundAssetRef, hydrate_entry, validate_params,
};

// Re-export GLB types
pub use glb::{
    AnimationChannel, AnimationPath, GlbError, ImportedAnimationClip, ImportedCameraProjection,
    ImportedExtras, ImportedMesh, ImportedScene, ImportedSceneAnchor, ImportedSceneCamera,
    ImportedSceneDirectionalLight, ImportedSceneMaterial, ImportedSceneMeshNode,
    ImportedSceneMetadata, ImportedVertex, SceneAssetRef, load_glb_mesh, load_glb_scene,
};
