// weaveback-api/src/mcp.rs
// I'd Really Rather You Didn't edit this generated file.

mod helpers;
mod run;

pub use run::run_mcp;

#[cfg(test)]
pub(crate) use run::get_or_spawn_lsp;

#[cfg(test)]
mod tests;

