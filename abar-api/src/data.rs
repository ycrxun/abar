//! Model-context data classification and projection contracts.

use crate::context::RequestContext;
use crate::provider::{BoxFuture, ProviderError};
use serde::{Deserialize, Serialize};

/// One classified field in a JSON payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataFieldClassification {
    pub path: String,
    pub data_class: String,
}

/// Classification returned for one JSON payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataClassification {
    pub fields: Vec<DataFieldClassification>,
}

/// Data-class policy applied to tool results and model output.
pub trait DataPolicy: Send + Sync {
    /// Classifies JSON fields before they enter model context.
    fn classify<'a>(
        &'a self,
        context: &'a RequestContext,
        payload_json: &'a str,
    ) -> BoxFuture<'a, Result<DataClassification, ProviderError>>;

    /// Projects a payload to the classes allowed for model context.
    fn project_for_model<'a>(
        &'a self,
        context: &'a RequestContext,
        payload_json: &'a str,
        classification: &'a DataClassification,
        allowed_data_classes: &'a [String],
        denied_data_classes: &'a [String],
    ) -> BoxFuture<'a, Result<String, ProviderError>>;

    /// Fails closed if model output or a persisted projection contains denied data.
    fn guard_output<'a>(
        &'a self,
        context: &'a RequestContext,
        output: &'a str,
        denied_data_classes: &'a [String],
    ) -> BoxFuture<'a, Result<String, ProviderError>>;
}

#[cfg(test)]
mod tests {
    use super::{DataClassification, DataFieldClassification};

    #[test]
    fn classification_round_trips() {
        let value = DataClassification {
            fields: vec![DataFieldClassification {
                path: "/secretValue".to_owned(),
                data_class: "restricted".to_owned(),
            }],
        };
        let encoded = serde_json::to_string(&value).unwrap();
        let decoded: DataClassification = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}
