use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedResourceAction {
    NoOp,
    Create,
    Update,
    Replace,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceDiffDecision {
    Decided(PlannedResourceAction),
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceDiff {
    pub decision: ResourceDiffDecision,
    pub changed_inputs: Vec<String>,
    pub unknown_outputs: Vec<String>,
}

impl ResourceDiff {
    pub fn new(action: PlannedResourceAction) -> Self {
        Self {
            decision: ResourceDiffDecision::Decided(action),
            changed_inputs: Vec::new(),
            unknown_outputs: Vec::new(),
        }
    }

    pub fn deferred() -> Self {
        Self {
            decision: ResourceDiffDecision::Deferred,
            changed_inputs: Vec::new(),
            unknown_outputs: Vec::new(),
        }
    }

    pub fn with_changed_input(mut self, input: impl Into<String>) -> Self {
        self.changed_inputs.push(input.into());
        self
    }

    pub fn with_unknown_output(mut self, output: impl Into<String>) -> Self {
        self.unknown_outputs.push(output.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderOperationError {
    pub message: String,
}

impl ProviderOperationError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn unsupported(operation: impl Into<String>) -> Self {
        Self::new(format!(
            "unsupported provider operation `{}`",
            operation.into()
        ))
    }
}

impl fmt::Display for ProviderOperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "provider operation failed: {}", self.message)
    }
}

impl Error for ProviderOperationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_resource_diff() {
        let diff = ResourceDiff::new(PlannedResourceAction::Update)
            .with_changed_input("name")
            .with_unknown_output("urn");

        assert_eq!(
            diff.decision,
            ResourceDiffDecision::Decided(PlannedResourceAction::Update)
        );
        assert_eq!(diff.changed_inputs, vec!["name"]);
        assert_eq!(diff.unknown_outputs, vec!["urn"]);
    }

    #[test]
    fn creates_deferred_resource_diff() {
        let diff = ResourceDiff::deferred()
            .with_changed_input("name")
            .with_unknown_output("urn");

        assert_eq!(diff.decision, ResourceDiffDecision::Deferred);
        assert_eq!(diff.changed_inputs, vec!["name"]);
        assert_eq!(diff.unknown_outputs, vec!["urn"]);
    }

    #[test]
    fn formats_operation_error() {
        assert_eq!(
            ProviderOperationError::new("api request failed").to_string(),
            "provider operation failed: api request failed"
        );
    }

    #[test]
    fn creates_unsupported_operation_error() {
        assert_eq!(
            ProviderOperationError::unsupported("create").message,
            "unsupported provider operation `create`"
        );
    }
}
