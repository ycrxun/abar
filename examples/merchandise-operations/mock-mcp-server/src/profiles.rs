use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MockProfile {
    Normal,
    Stale,
    Partial,
    Sensitive,
    Oversized,
    Slow,
    Unavailable,
}

impl MockProfile {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "normal" => Ok(Self::Normal),
            "stale" => Ok(Self::Stale),
            "partial" => Ok(Self::Partial),
            "sensitive" => Ok(Self::Sensitive),
            "oversized" | "paginated" => Ok(Self::Oversized),
            "slow" => Ok(Self::Slow),
            "unavailable" | "down" => Ok(Self::Unavailable),
            _ => Err(format!("unknown mock profile: {value}")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Stale => "stale",
            Self::Partial => "partial",
            Self::Sensitive => "sensitive",
            Self::Oversized => "oversized",
            Self::Slow => "slow",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn list() -> &'static [&'static str] {
        &[
            "normal",
            "stale",
            "partial",
            "sensitive",
            "oversized",
            "slow",
            "unavailable",
        ]
    }

    pub fn delay(self) -> Duration {
        let configured = std::env::var("ABAR_MOCK_TOOL_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok());
        Duration::from_millis(configured.unwrap_or(if self == Self::Slow { 1_200 } else { 0 }))
    }

    pub fn transform(self, result: &mut Value) {
        match self {
            Self::Normal | Self::Slow | Self::Unavailable => {}
            Self::Stale => {
                result["dataAsOf"] = Value::String("2026-11-01T08:00:00+08:00".to_owned());
                result["freshnessStatus"] = Value::String("STALE".to_owned());
                result["freshness"] = json!({
                    "status": "STALE",
                    "ageHours": 361.5,
                    "requiresRefresh": true
                });
            }
            Self::Partial => partial(result),
            Self::Sensitive => sensitive(result),
            Self::Oversized => {
                result["pageInfo"] = json!({
                    "pageSize": 200,
                    "maxPageSize": 200,
                    "truncated": true,
                    "nextCursor": "mock-page-2"
                });
            }
        }
    }
}

fn partial(result: &mut Value) {
    let (field, returned, dropped) =
        if let Some(items) = result.get_mut("items").and_then(Value::as_array_mut) {
            let dropped = drop_half(items);
            ("items", items.len(), dropped)
        } else if let Some(recommendations) = result
            .get_mut("recommendations")
            .and_then(Value::as_array_mut)
        {
            let dropped = drop_half(recommendations);
            ("recommendations", recommendations.len(), dropped)
        } else {
            ("", 0, 0)
        };
    if field == "items" {
        result["returned"] = json!(returned);
    } else if field == "recommendations" {
        result["summary"]["returnedRecommendations"] = json!(returned);
    }
    result["dataQuality"] = json!({
        "status": "PARTIAL",
        "droppedItems": dropped,
        "missingPartitions": ["ST011", "ST035", "ST053"],
        "mayProduceIncompleteRecommendations": true
    });
}

fn drop_half(items: &mut Vec<Value>) -> usize {
    let keep = items.len().div_ceil(2);
    let dropped = items.len() - keep;
    items.truncate(keep);
    dropped
}

fn sensitive(result: &mut Value) {
    for key in ["items", "recommendations"] {
        if let Some(items) = result.get_mut(key).and_then(Value::as_array_mut) {
            for (index, item) in items.iter_mut().enumerate() {
                item["unitCost"] = json!(199.0 + index as f64);
                item["grossMargin"] = json!(0.42 + index as f64 / 100.0);
                item["supplierPrice"] = json!(128.0 + index as f64);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MockProfile;
    use serde_json::json;

    #[test]
    fn partial_and_sensitive_profiles_change_existing_arrays() {
        let mut result = json!({
            "dataAsOf": "2026-11-16T09:30:00+08:00",
            "returned": 4,
            "items": [{"id": 1}, {"id": 2}, {"id": 3}, {"id": 4}]
        });
        MockProfile::Partial.transform(&mut result);
        assert_eq!(result["items"].as_array().unwrap().len(), 2);
        assert_eq!(result["returned"], 2);
        MockProfile::Sensitive.transform(&mut result);
        assert_eq!(result["items"][0]["unitCost"], 199.0);
    }
}
