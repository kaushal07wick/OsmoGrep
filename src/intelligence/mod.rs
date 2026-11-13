pub mod cli;
pub mod git;
pub mod mcp;
pub mod predict;
pub mod reason;
pub mod verify;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(test)]
mod tests;
