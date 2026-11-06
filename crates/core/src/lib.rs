//! Desktop side of OpenCamera that pairs a phone, receives its video and feeds the virtual camera.

pub mod decoder;
mod engine;
pub mod net;
pub mod pairing;
pub mod pipeline;
pub mod process;
pub mod protocol;
mod server;
mod session;

pub use engine::*;
pub use opencamera_vcam as vcam;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
