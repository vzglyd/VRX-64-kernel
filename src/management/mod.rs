//! Management types shared between the native HTTP server and the web editor.
//!
//! Centralising these types in the kernel ensures that playlist validation,
//! hydration logic, and secrets shape are never duplicated across platforms.
//!
//! # Modules
//!
//! - [`secrets`]: [`SecretsStore`] for persisting API keys and other sensitive values.
//! - [`hydration`]: [`HydratedPlaylistEntry`] and [`hydrate_entry`] for combining
//!   playlist entries with their manifest metadata and resolving display defaults.
//! - [`library`]: [`SlideLibraryEntry`] for listing available `.vzglyd` bundles.

pub mod hydration;
pub mod library;
pub mod secrets;

pub use crate::manifest::SoundAssetRef;
pub use hydration::{
    ENGINE_DEFAULT_DURATION_SECS, HydratedPlaylistEntry, hydrate_entry, validate_params,
};
pub use library::SlideLibraryEntry;
pub use secrets::{SECRETS_FILENAME, SecretsStore};
