#![forbid(unsafe_code)]

mod config;
mod state;

pub use config::{
    ConfigError, Endpoint, Peer, Profile, ProfileSummary, SecretKey, MAX_CONFIG_BYTES,
};
pub use state::{ConnectionState, Mode, StateError};
