//! Helpers shared by the tools in this crate.

use crate::Tool;
use crate::*;

/// Look a tool up by id so `run` bodies stop depending on the registry order.
pub(crate) fn tool(id: &str) -> &'static Tool {
    TOOLS
        .iter()
        .find(|t| t.id == id)
        .expect("run fns only ask for their own tool")
}
