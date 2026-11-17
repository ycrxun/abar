//! Reusable provider contract validators for adapter tests.

use crate::data::DataClassification;
use crate::knowledge::{KnowledgeDocument, KnowledgeQuery};
use crate::provider::{ModelRequest, ModelResponse, ToolDescriptor};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

/// One failed or successful provider contract assertion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConformanceIssue {
    pub check: &'static str,
    pub detail: String,
}

/// Validates a ModelProvider response against its request.
#[must_use]
pub fn validate_model_response(
    request: &ModelRequest,
    response: &ModelResponse,
) -> Vec<ConformanceIssue> {
    let mut issues = Vec::new();
    if response.served_model_id != request.model_id {
        issues.push(issue(
            "model.identity",
            format!(
                "served model {} does not match requested {}",
                response.served_model_id, request.model_id
            ),
        ));
    }
    if response.output.trim().is_empty() && response.tool_intents.is_empty() {
        issues.push(issue(
            "model.non_empty",
            "response has neither text output nor Tool intent".to_owned(),
        ));
    }
    let declared = request
        .tools
        .iter()
        .map(|tool| tool.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for intent in &response.tool_intents {
        if !declared.contains(intent.tool_id.as_str()) {
            issues.push(issue(
                "model.declared_tool",
                format!("Tool intent {} was not declared", intent.tool_id),
            ));
        }
        if !seen.insert(intent.tool_id.as_str()) {
            issues.push(issue(
                "model.unique_tool_intent",
                format!("Tool intent {} appears more than once", intent.tool_id),
            ));
        }
        if serde_json::from_str::<Value>(&intent.arguments_json).is_err() {
            issues.push(issue(
                "model.tool_arguments_json",
                format!("Tool intent {} has invalid JSON arguments", intent.tool_id),
            ));
        }
    }
    issues
}

/// Validates Tool descriptors returned by a ToolRegistry.
#[must_use]
pub fn validate_tool_descriptors(descriptors: &[ToolDescriptor]) -> Vec<ConformanceIssue> {
    let mut issues = Vec::new();
    let mut ids = BTreeSet::new();
    for descriptor in descriptors {
        if descriptor.id.trim().is_empty() {
            issues.push(issue(
                "tool.id",
                "Tool descriptor has an empty id".to_owned(),
            ));
        } else if !ids.insert(descriptor.id.as_str()) {
            issues.push(issue(
                "tool.unique_id",
                format!("Tool {} appears more than once", descriptor.id),
            ));
        }
        if descriptor.version.trim().is_empty() {
            issues.push(issue(
                "tool.version",
                format!("Tool {} has an empty version", descriptor.id),
            ));
        }
        if !valid_digest(&descriptor.schema_digest) {
            issues.push(issue(
                "tool.schema_digest",
                format!("Tool {} has invalid schema digest", descriptor.id),
            ));
        }
        for (label, schema) in [
            ("input", &descriptor.input_schema_json),
            ("output", &descriptor.output_schema_json),
        ] {
            if !matches!(serde_json::from_str::<Value>(schema), Ok(Value::Object(_))) {
                issues.push(issue(
                    "tool.schema_json",
                    format!("Tool {} has invalid {label} Schema", descriptor.id),
                ));
            }
        }
    }
    issues
}

/// Validates Knowledge documents against one retrieval request.
#[must_use]
pub fn validate_knowledge_response(
    query: &KnowledgeQuery,
    documents: &[KnowledgeDocument],
) -> Vec<ConformanceIssue> {
    let mut issues = Vec::new();
    if query.top_k == 0 {
        issues.push(issue(
            "knowledge.query_top_k",
            "Knowledge query topK must be positive".to_owned(),
        ));
    } else if documents.len() > query.top_k as usize {
        issues.push(issue(
            "knowledge.top_k",
            format!(
                "Provider returned {} documents for topK {}",
                documents.len(),
                query.top_k
            ),
        ));
    }
    let mut ids = BTreeSet::new();
    for document in documents {
        if document.source != query.source {
            issues.push(issue(
                "knowledge.source",
                format!(
                    "Document source {} does not match requested {}",
                    document.source, query.source
                ),
            ));
        }
        if document.document_id.trim().is_empty() || !ids.insert(document.document_id.as_str()) {
            issues.push(issue(
                "knowledge.document_id",
                "Knowledge document ID is empty or duplicated".to_owned(),
            ));
        }
        if document.version.trim().is_empty() || document.retrieved_at.trim().is_empty() {
            issues.push(issue(
                "knowledge.evidence",
                format!(
                    "Document {} is missing version or retrieval time",
                    document.document_id
                ),
            ));
        }
        if !valid_digest(&document.digest) {
            issues.push(issue(
                "knowledge.digest",
                format!("Document {} has invalid digest", document.document_id),
            ));
        }
    }
    issues
}

/// Validates that denied classified fields were removed from a projection.
#[must_use]
pub fn validate_data_projection(
    classification: &DataClassification,
    denied_data_classes: &[String],
    projected_json: &str,
) -> Vec<ConformanceIssue> {
    let mut issues = Vec::new();
    let denied = denied_data_classes
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for field in &classification.fields {
        if field.path.trim().is_empty() || field.data_class.trim().is_empty() {
            issues.push(issue(
                "data.classification",
                "Classified field has an empty path or data class".to_owned(),
            ));
        }
    }
    let projected: Value = match serde_json::from_str(projected_json) {
        Ok(value) => value,
        Err(error) => {
            issues.push(issue(
                "data.projection_json",
                format!("Projected payload is invalid JSON: {error}"),
            ));
            return issues;
        }
    };
    for field in &classification.fields {
        if denied.contains(field.data_class.as_str()) && contains_path(&projected, &field.path) {
            issues.push(issue(
                "data.denied_field_removed",
                format!("Denied field {} remains in model projection", field.path),
            ));
        }
    }
    issues
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    })
}

fn contains_path(value: &Value, path: &str) -> bool {
    if path.starts_with('/') {
        return value.pointer(path).is_some();
    }
    let segments = path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    contains_segments(value, &segments)
}

fn contains_segments(value: &Value, segments: &[&str]) -> bool {
    match value {
        Value::Object(map) => segments.first().is_some_and(|segment| {
            map.get(*segment).is_some_and(|value| {
                segments.len() == 1 || contains_segments(value, &segments[1..])
            })
        }),
        Value::Array(values) => values
            .iter()
            .any(|value| contains_segments(value, segments)),
        _ => false,
    }
}

fn issue(check: &'static str, detail: String) -> ConformanceIssue {
    ConformanceIssue { check, detail }
}

#[cfg(test)]
mod tests {
    use super::{validate_data_projection, validate_model_response, validate_tool_descriptors};
    use crate::data::{DataClassification, DataFieldClassification};
    use crate::provider::{
        ModelRequest, ModelResponse, ModelTool, ToolDescriptor, ToolEffect, ToolIntent,
    };

    #[test]
    fn accepts_consistent_model_and_tool_contracts() {
        let request = ModelRequest {
            model_id: "model-1".to_owned(),
            input: "query".to_owned(),
            tools: vec![ModelTool {
                id: "catalog.query-example".to_owned(),
                description: "query".to_owned(),
                input_schema_json: "{}".to_owned(),
            }],
            tool_results: Vec::new(),
        };
        let response = ModelResponse {
            served_model_id: "model-1".to_owned(),
            output: String::new(),
            tool_intents: vec![ToolIntent {
                tool_id: "catalog.query-example".to_owned(),
                arguments_json: "{}".to_owned(),
            }],
            usage: Default::default(),
        };
        assert!(validate_model_response(&request, &response).is_empty());
        assert!(
            validate_tool_descriptors(&[ToolDescriptor {
                id: "catalog.query-example".to_owned(),
                version: "1".to_owned(),
                description: "query".to_owned(),
                input_schema_json: "{}".to_owned(),
                output_schema_json: "{}".to_owned(),
                effect: ToolEffect::Read,
                schema_digest: format!("sha256:{}", "a".repeat(64)),
            }])
            .is_empty()
        );
    }

    #[test]
    fn rejects_denied_field_in_projection() {
        let classification = DataClassification {
            fields: vec![DataFieldClassification {
                path: "/secretValue".to_owned(),
                data_class: "restricted".to_owned(),
            }],
        };
        let issues = validate_data_projection(
            &classification,
            &["restricted".to_owned()],
            r#"{"secretValue":12}"#,
        );
        assert_eq!(issues[0].check, "data.denied_field_removed");
    }
}
