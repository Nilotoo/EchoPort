//! Scripting surface.
//!
//! The `scripting` feature (default) provides the full Lua config/plugin
//! implementation (`real.rs`). Lean builds without the feature use a tiny
//! compatible stub so the rest of the app compiles unchanged while removing
//! the embedded Lua runtime and its dependencies.

pub mod keys;
pub use keys::{BindingAction, KeyBinding, KeyChord};

#[cfg(feature = "scripting")]
#[path = "real.rs"]
mod real;
#[cfg(feature = "scripting")]
pub use real::*;

#[cfg(not(feature = "scripting"))]
#[path = "stub.rs"]
mod stub;
#[cfg(not(feature = "scripting"))]
pub use stub::*;
