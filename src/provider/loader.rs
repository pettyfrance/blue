use super::{ProviderError, ProviderRegistry};

pub trait ProviderLoader {
    fn load(&self, registry: &mut ProviderRegistry) -> Result<(), ProviderError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BuiltinProviderLoader;

impl BuiltinProviderLoader {
    pub fn new() -> Self {
        Self
    }
}

impl ProviderLoader for BuiltinProviderLoader {
    fn load(&self, _registry: &mut ProviderRegistry) -> Result<(), ProviderError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_loader_currently_registers_no_providers() {
        let loader = BuiltinProviderLoader::new();
        let mut registry = ProviderRegistry::new();

        loader.load(&mut registry).unwrap();

        assert_eq!(registry.provider_count(), 0);
    }
}
