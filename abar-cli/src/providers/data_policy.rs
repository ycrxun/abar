use super::message_config::DataFieldRule;
use abar_api::context::RequestContext;
use abar_api::data::{DataClassification, DataFieldClassification, DataPolicy};
use abar_api::error::ErrorCode;
use abar_api::provider::{BoxFuture, ProviderError};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub(super) struct ConfiguredDataPolicy {
    fields: Vec<DataFieldRule>,
    sensitive_values: Arc<Mutex<BTreeSet<String>>>,
}

impl ConfiguredDataPolicy {
    pub(super) fn new(fields: Vec<DataFieldRule>) -> Self {
        Self {
            fields,
            sensitive_values: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    fn classification(&self, payload: &str) -> Result<DataClassification, ProviderError> {
        let value: Value = serde_json::from_str(payload).map_err(invalid_json)?;
        let mut fields = Vec::new();
        collect_classifications(&value, "", &self.fields, &mut fields);
        Ok(DataClassification { fields })
    }
}

impl DataPolicy for ConfiguredDataPolicy {
    fn classify<'a>(
        &'a self,
        _context: &'a RequestContext,
        payload_json: &'a str,
    ) -> BoxFuture<'a, Result<DataClassification, ProviderError>> {
        Box::pin(async move { self.classification(payload_json) })
    }

    fn project_for_model<'a>(
        &'a self,
        _context: &'a RequestContext,
        payload_json: &'a str,
        _classification: &'a DataClassification,
        _allowed_data_classes: &'a [String],
        denied_data_classes: &'a [String],
    ) -> BoxFuture<'a, Result<String, ProviderError>> {
        let result = (|| {
            let mut value: Value = serde_json::from_str(payload_json).map_err(invalid_json)?;
            project_value(
                &mut value,
                "",
                &self.fields,
                denied_data_classes,
                &self.sensitive_values,
            );
            serde_json::to_string(&value).map_err(invalid_json)
        })();
        Box::pin(async move { result })
    }

    fn guard_output<'a>(
        &'a self,
        _context: &'a RequestContext,
        output: &'a str,
        _denied_data_classes: &'a [String],
    ) -> BoxFuture<'a, Result<String, ProviderError>> {
        let contains = (|| {
            Ok::<_, ProviderError>(
                self.sensitive_values
                    .lock()
                    .map_err(|_| provider_error("sensitive value tracker is poisoned"))?
                    .iter()
                    .any(|value| value.len() >= 3 && output.contains(value)),
            )
        })();
        Box::pin(async move {
            if contains? {
                Err(provider_error("model output contains denied data"))
            } else {
                Ok(output.to_owned())
            }
        })
    }
}

fn collect_classifications(
    value: &Value,
    prefix: &str,
    rules: &[DataFieldRule],
    output: &mut Vec<DataFieldClassification>,
) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                if let Some(rule) = rules
                    .iter()
                    .find(|rule| rule.path == path || rule.path == *key)
                {
                    output.push(DataFieldClassification {
                        path: path.clone(),
                        data_class: rule.data_class.clone(),
                    });
                }
                collect_classifications(value, &path, rules, output);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_classifications(value, prefix, rules, output);
            }
        }
        _ => {}
    }
}

fn project_value(
    value: &mut Value,
    prefix: &str,
    rules: &[DataFieldRule],
    denied: &[String],
    sensitive_values: &Mutex<BTreeSet<String>>,
) {
    match value {
        Value::Object(map) => {
            let keys = map.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                let denied_field = rules.iter().any(|rule| {
                    (rule.path == path || rule.path == key) && denied.contains(&rule.data_class)
                });
                if denied_field {
                    if let Some(value) = map.remove(&key)
                        && let Ok(mut values) = sensitive_values.lock()
                    {
                        collect_scalars(&value, &mut values);
                    }
                } else if let Some(value) = map.get_mut(&key) {
                    project_value(value, &path, rules, denied, sensitive_values);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                project_value(value, prefix, rules, denied, sensitive_values);
            }
        }
        _ => {}
    }
}

fn collect_scalars(value: &Value, output: &mut BTreeSet<String>) {
    match value {
        Value::String(value) => {
            output.insert(value.clone());
        }
        Value::Number(value) => {
            output.insert(value.to_string());
        }
        Value::Bool(value) => {
            output.insert(value.to_string());
        }
        Value::Array(values) => {
            for value in values {
                collect_scalars(value, output);
            }
        }
        Value::Object(map) => {
            for value in map.values() {
                collect_scalars(value, output);
            }
        }
        Value::Null => {}
    }
}

fn invalid_json(error: serde_json::Error) -> ProviderError {
    provider_error(error.to_string())
}

fn provider_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ErrorCode::InvalidRequest, message, false)
}

#[cfg(test)]
mod tests {
    use super::ConfiguredDataPolicy;
    use crate::providers::message_config::DataFieldRule;
    use abar_api::conformance::validate_data_projection;
    use abar_api::context::{RequestContext, TenantContext};
    use abar_api::data::DataPolicy;

    #[test]
    fn configured_policy_satisfies_projection_contract() {
        let policy = ConfiguredDataPolicy::new(vec![DataFieldRule {
            path: "secretValue".to_owned(),
            data_class: "restricted".to_owned(),
        }]);
        let context = RequestContext::new("req", TenantContext::new("tenant"));
        let payload = r#"{"id":"A","secretValue":12}"#;
        let classification = block_on(policy.classify(&context, payload)).unwrap();
        let denied = vec!["restricted".to_owned()];
        let projected =
            block_on(policy.project_for_model(&context, payload, &classification, &[], &denied))
                .unwrap();
        assert!(validate_data_projection(&classification, &denied, &projected).is_empty());
        assert!(!projected.contains("secretValue"));
    }

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => std::thread::yield_now(),
            }
        }
    }
}
