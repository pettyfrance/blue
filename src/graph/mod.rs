//! Desired declaration graph.

mod builder;
mod desired;
mod model;

pub use builder::{DesiredGraphBuilder, GraphBuildError};
pub use desired::{DesiredGraph, GraphCycle, GraphLookupError};
pub use model::{Declaration, DeclarationId};
