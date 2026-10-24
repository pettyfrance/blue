use std::collections::BTreeMap;

/// Named fields accepted or exposed by a provider or resource type.
/// Input and output schemas are separate instances of this same structure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    pub fields: BTreeMap<String, FieldSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSchema {
    pub ty: ValueType,
    /// Whether the field must be present. Optional fields may be absent;
    /// this flag does not assign null, a default, or an unknown value.
    pub required: bool,
    /// Marks values requiring redaction. Consumers must enforce redaction;
    /// this metadata alone does not protect values or encrypt storage.
    pub sensitive: bool,
}

/// Runtime value shapes, not configuration expressions or concrete values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueType {
    String,
    Integer,
    Float,
    Bool,
    Array(Box<ValueType>),
    Object(Schema),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(ty: ValueType, required: bool) -> FieldSchema {
        FieldSchema {
            ty,
            required,
            sensitive: false,
        }
    }

    #[test]
    fn empty_schema_has_no_fields() {
        assert!(Schema::default().fields.is_empty());
    }

    #[test]
    fn describes_provider_configuration_and_sensitive_credentials() {
        let schema = Schema {
            fields: BTreeMap::from([
                ("endpoint".into(), field(ValueType::String, false)),
                (
                    "token".into(),
                    FieldSchema {
                        ty: ValueType::String,
                        required: true,
                        sensitive: true,
                    },
                ),
            ]),
        };
        assert!(schema.fields["token"].required);
        assert!(schema.fields["token"].sensitive);
        assert!(!schema.fields["endpoint"].required);
        assert!(!schema.fields["endpoint"].sensitive);
    }

    #[test]
    fn represents_all_scalar_types_and_arrays() {
        let schema = Schema {
            fields: BTreeMap::from([
                ("name".into(), field(ValueType::String, true)),
                ("count".into(), field(ValueType::Integer, true)),
                ("ratio".into(), field(ValueType::Float, false)),
                ("enabled".into(), field(ValueType::Bool, false)),
                (
                    "labels".into(),
                    field(ValueType::Array(Box::new(ValueType::String)), false),
                ),
            ]),
        };
        assert_eq!(schema.fields["name"].ty, ValueType::String);
        assert_eq!(schema.fields["count"].ty, ValueType::Integer);
        assert_eq!(schema.fields["ratio"].ty, ValueType::Float);
        assert_eq!(schema.fields["enabled"].ty, ValueType::Bool);
        assert_eq!(
            schema.fields["labels"].ty,
            ValueType::Array(Box::new(ValueType::String))
        );
    }

    #[test]
    fn describes_nested_output_objects_and_arrays_of_objects() {
        let network = Schema {
            fields: BTreeMap::from([("public_ip".into(), field(ValueType::String, false))]),
        };
        let schema = Schema {
            fields: BTreeMap::from([
                ("id".into(), field(ValueType::String, true)),
                (
                    "network".into(),
                    field(ValueType::Object(network.clone()), true),
                ),
                (
                    "interfaces".into(),
                    field(
                        ValueType::Array(Box::new(ValueType::Object(network.clone()))),
                        false,
                    ),
                ),
            ]),
        };
        let ValueType::Object(nested) = &schema.fields["network"].ty else {
            panic!("expected object")
        };
        assert!(!nested.fields["public_ip"].required);
        assert_eq!(
            schema.fields["interfaces"].ty,
            ValueType::Array(Box::new(ValueType::Object(network)))
        );
    }

    #[test]
    fn input_and_output_contracts_are_independent() {
        let inputs = Schema {
            fields: BTreeMap::from([("region".into(), field(ValueType::String, true))]),
        };
        let outputs = Schema {
            fields: BTreeMap::from([("id".into(), field(ValueType::String, true))]),
        };
        assert!(!inputs.fields.contains_key("id"));
        assert!(!outputs.fields.contains_key("region"));
        assert_eq!(outputs.clone(), outputs);
    }
}
