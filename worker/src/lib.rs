pub mod page;
pub mod platform;

#[cfg(target_arch = "wasm32")]
mod worker_entry;
