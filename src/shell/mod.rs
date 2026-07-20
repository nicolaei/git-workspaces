//! All I/O lives here. Gathers real state, calls `domain` to decide what to
//! do, executes the decision, prints the result. Decides nothing
//! business-relevant itself.

pub mod error;
pub mod exec;
pub mod fs;
pub mod git;
