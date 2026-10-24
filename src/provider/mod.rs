//! Provider contracts and schema definitions.

mod loader;
mod registry;
mod schema;
mod traits;

pub use loader::{BuiltinProviderLoader, ProviderLoadError};
pub use registry::{ProviderRegistrationError, ProviderRegistry};
pub use schema::{FieldSchema, Schema, ValueType};
pub use traits::{Provider, ResourceType};
