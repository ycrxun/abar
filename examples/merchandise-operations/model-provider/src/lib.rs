use abar_api::context::RequestContext;
use abar_api::provider::{
    BoxFuture, ModelProvider, ModelRequest, ModelResponse, ProviderError, ToolIntent,
};

/// Deterministic provider used by the merchandise operations example.
#[derive(Debug)]
pub struct BusinessDemoModelProvider {
    model_id: String,
    delay: std::time::Duration,
}

impl BusinessDemoModelProvider {
    /// Creates the deterministic business demo provider.
    #[must_use]
    pub fn new(model_id: impl Into<String>, delay: std::time::Duration) -> Self {
        Self {
            model_id: model_id.into(),
            delay,
        }
    }
}

impl ModelProvider for BusinessDemoModelProvider {
    fn complete<'a>(
        &'a self,
        _context: &'a RequestContext,
        request: &'a ModelRequest,
    ) -> BoxFuture<'a, Result<ModelResponse, ProviderError>> {
        let request_text = user_request(&request.input);
        let request_operation = operation_from_text(request_text);
        let plan_operation = operation_from_plan_type(&request.input);
        let operation = plan_operation.or(request_operation);
        let region = ["华东", "华南", "华北", "西南", "华中"]
            .into_iter()
            .find(|region| request_text.contains(region))
            .unwrap_or("华东");
        let model_id = self.model_id.clone();
        let recommendation_tool_id = operation
            .and_then(|operation| find_tool(&request.tools, operation.recommendation_suffix()));
        let write_tool_id =
            operation.and_then(|operation| find_tool(&request.tools, operation.write_suffix()));
        let delay = self.delay;
        let plan_digest = json_string_field(&request.input, "planDigest");
        let has_plan = plan_digest.is_some();
        let has_matching_plan = plan_digest.is_some()
            && request_operation == plan_operation
            && request_operation.is_some();
        let required_scopes = json_string_array_field(&request.input, "requiredScopes")
            .unwrap_or_else(|| vec![region.to_owned()]);
        let order_id = json_string_field(&request.input, "orderId");
        let wants_execution = request_text.contains("审批")
            || request_text.contains("创建")
            || request_text.contains("确认执行");
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            if let Some(order_id) = order_id {
                Ok(ModelResponse {
                    served_model_id: model_id,
                    output: format!(
                        "已创建业务单 {order_id}。方案审批、工具参数和幂等结果均已记录。"
                    ),
                    tool_intents: Vec::new(),
                    usage: Default::default(),
                })
            } else if let (Some(plan_digest), Some(write_tool_id), true) =
                (plan_digest, write_tool_id, wants_execution)
            {
                Ok(ModelResponse {
                    served_model_id: model_id,
                    output: String::new(),
                    tool_intents: vec![ToolIntent {
                        tool_id: write_tool_id,
                        arguments_json: serde_json::json!({
                            "planDigest": plan_digest,
                            "requiredScopes": required_scopes
                        })
                        .to_string(),
                    }],
                    usage: Default::default(),
                })
            } else if has_matching_plan && !wants_execution {
                Ok(planned_response(model_id, region))
            } else if request_operation.is_some() && (!wants_execution || !has_plan) {
                let Some(recommendation_tool_id) = recommendation_tool_id else {
                    return Ok(clarification_response(
                        model_id,
                        "当前 Agent 未绑定该业务的计划工具，请确认业务类型或权限范围。",
                    ));
                };
                Ok(ModelResponse {
                    served_model_id: model_id,
                    output: String::new(),
                    tool_intents: vec![ToolIntent {
                        tool_id: recommendation_tool_id,
                        arguments_json: serde_json::json!({
                            "region": region,
                            "planningWindowDays": 7,
                            "maxRecommendations": 6
                        })
                        .to_string(),
                    }],
                    usage: Default::default(),
                })
            } else if json_string_field(&request.input, "planDigest").is_some() {
                Ok(planned_response(model_id, region))
            } else {
                Ok(clarification_response(
                    model_id,
                    "请明确本次要做配货、补货还是门店间调拨。例如：生成华东区域补货计划。",
                ))
            }
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Operation {
    Allocation,
    Replenishment,
    Transfer,
}

impl Operation {
    fn recommendation_suffix(self) -> &'static str {
        match self {
            Self::Allocation => "calculate_allocation_plan",
            Self::Replenishment => "calculate_replenishment_plan",
            Self::Transfer => "calculate_transfer_recommendations",
        }
    }

    fn write_suffix(self) -> &'static str {
        match self {
            Self::Allocation => "create_allocation_order",
            Self::Replenishment => "create_replenishment_order",
            Self::Transfer => "create_transfer_order",
        }
    }

    fn plan_type(self) -> &'static str {
        match self {
            Self::Allocation => "allocation",
            Self::Replenishment => "replenishment",
            Self::Transfer => "transfer",
        }
    }
}

fn operation_from_text(text: &str) -> Option<Operation> {
    let compact = text.to_ascii_lowercase();
    let mut matches = Vec::new();
    if compact.contains("配货") || compact.contains("首铺") || compact.contains("allocation") {
        matches.push(Operation::Allocation);
    }
    if compact.contains("补货") || compact.contains("replenishment") {
        matches.push(Operation::Replenishment);
    }
    if compact.contains("调拨") || compact.contains("transfer") {
        matches.push(Operation::Transfer);
    }
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

fn operation_from_plan_type(input: &str) -> Option<Operation> {
    [
        Operation::Allocation,
        Operation::Replenishment,
        Operation::Transfer,
    ]
    .into_iter()
    .find(|operation| input.contains(&format!("\"planType\":\"{}\"", operation.plan_type())))
}

fn find_tool(tools: &[abar_api::provider::ModelTool], suffix: &str) -> Option<String> {
    tools
        .iter()
        .find(|tool| tool.id.ends_with(suffix))
        .map(|tool| tool.id.clone())
}

fn planned_response(model_id: String, region: &str) -> ModelResponse {
    ModelResponse {
        served_model_id: model_id,
        output: format!(
            "已基于模拟数据生成{region}区域业务计划。请查看工具结果中的数据时间、规则版本、逐条数量与原因码。"
        ),
        tool_intents: Vec::new(),
        usage: Default::default(),
    }
}

fn clarification_response(model_id: String, message: &str) -> ModelResponse {
    ModelResponse {
        served_model_id: model_id,
        output: message.to_owned(),
        tool_intents: Vec::new(),
        usage: Default::default(),
    }
}

fn json_string_field(input: &str, name: &str) -> Option<String> {
    let key = format!("\"{name}\"");
    let key_start = input.find(&key)?;
    let value = input.get(key_start + key.len()..)?.trim_start();
    let value = value.strip_prefix(':')?.trim_start();
    let value = value.strip_prefix('"')?;
    let end = value.find('"')?;
    value.get(..end).map(str::to_owned)
}

fn json_string_array_field(input: &str, name: &str) -> Option<Vec<String>> {
    let key = format!("\"{name}\"");
    let key_start = input.find(&key)?;
    let value = input.get(key_start + key.len()..)?.trim_start();
    let value = value.strip_prefix(':')?.trim_start();
    let start = value.find('[')?;
    let end = value[start..].find(']')? + start + 1;
    serde_json::from_str(&value[start..end]).ok()
}

fn user_request(input: &str) -> &str {
    if let Some((_, value)) = input.rsplit_once("[user]\n") {
        return value.trim();
    }
    let user = input
        .rsplit_once("\nUser:\n")
        .map_or(input, |(_, value)| value);
    user.split_once("\nTool results:")
        .map_or(user, |(value, _)| value)
}
