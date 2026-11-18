#[cfg(all(windows, feature = "adapter-runtime"))]
pub mod adapters;
#[cfg(windows)]
pub mod pipe;
