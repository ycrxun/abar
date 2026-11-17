use serde_json::{Value, json};
use std::path::Path;

pub(crate) fn load_view(
    path: Option<&Path>,
    agent: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let view = match path {
        Some(path) => parse_view(&std::fs::read_to_string(path)?)?,
        None => default_view(agent),
    };
    resolve_view(view, agent)
}

fn parse_view(source: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(source)?;
    let view: Value = serde_json::to_value(yaml)?;
    if view.get("apiVersion").and_then(Value::as_str) != Some("webui/v1")
        || view.get("kind").and_then(Value::as_str) != Some("WebUIView")
        || view
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .is_none()
        || view
            .pointer("/metadata/title")
            .and_then(Value::as_str)
            .is_none()
        || view
            .pointer("/binding/agent")
            .and_then(Value::as_str)
            .is_none()
    {
        return Err(
            "view pack must use webui/v1 WebUIView and define binding.agent, metadata.name and metadata.title"
                .into(),
        );
    }
    Ok(view)
}

fn resolve_view(view: Value, agent: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let bound_agent = view
        .pointer("/binding/agent")
        .and_then(Value::as_str)
        .ok_or("WebUI View must bind an Agent")?;
    if bound_agent != agent {
        return Err(
            format!("WebUI View is bound to {bound_agent}, but WebUI serves {agent}").into(),
        );
    }
    let mut defaults = default_view(agent);
    merge(&mut defaults, view);
    Ok(defaults)
}

fn default_view(agent: &str) -> Value {
    json!({
        "apiVersion": "webui/v1",
        "kind": "WebUIView",
        "binding": {
            "agent": agent
        },
        "metadata": {
            "name": "default",
            "title": agent,
            "subtitle": "Session、审批与运行证据"
        },
        "task": {
            "title": "开始对话",
            "chatPlaceholder": "告诉 Agent 你想了解什么或要执行什么",
            "fields": [],
            "actions": []
        },
        "confirmation": {
            "label": "确认执行",
            "inputTemplate": "确认执行当前会话中已展示的业务方案。",
            "requestedSkills": []
        },
        "approval": {
            "enabled": true,
            "title": "等待人工审批",
            "approveLabel": "批准",
            "rejectLabel": "拒绝"
        },
        "context": {
            "enabled": true
        },
        "metrics": [],
        "panels": []
    })
}

fn merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                if let Some(current) = base.get_mut(&key) {
                    merge(current, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::{default_view, parse_view, resolve_view};
    use serde_json::Value;

    #[test]
    fn default_view_binds_the_webui_agent() {
        let view = default_view("example-assistant");
        assert_eq!(
            view.pointer("/binding/agent")
                .and_then(|value| value.as_str()),
            Some("example-assistant")
        );
        assert_eq!(
            view.pointer("/metadata/title")
                .and_then(|value| value.as_str()),
            Some("example-assistant")
        );
    }

    #[test]
    fn custom_view_must_declare_agent_binding() {
        let error =
            parse_view("apiVersion: webui/v1\nkind: WebUIView\nmetadata:\n  name: x\n  title: X\n")
                .unwrap_err();
        assert!(error.to_string().contains("binding.agent"));
    }

    #[test]
    fn missing_presentation_fields_use_defaults() {
        let custom = parse_view(
            "apiVersion: webui/v1\nkind: WebUIView\nbinding:\n  agent: assistant\nmetadata:\n  name: custom\n  title: Custom\n",
        )
        .unwrap();
        let view = resolve_view(custom, "assistant").unwrap();
        assert_eq!(
            view.pointer("/task/title").and_then(|value| value.as_str()),
            Some("开始对话")
        );
        assert_eq!(
            view.pointer("/metadata/title")
                .and_then(|value| value.as_str()),
            Some("Custom")
        );
        assert_eq!(view.pointer("/context/enabled"), Some(&Value::Bool(true)));
    }

    #[test]
    fn context_panel_can_be_disabled_by_a_view() {
        let custom = parse_view(
            "apiVersion: webui/v1\nkind: WebUIView\nbinding:\n  agent: assistant\nmetadata:\n  name: custom\n  title: Custom\ncontext:\n  enabled: false\n",
        )
        .unwrap();
        let view = resolve_view(custom, "assistant").unwrap();
        assert_eq!(view.pointer("/context/enabled"), Some(&Value::Bool(false)));
    }

    #[test]
    fn approval_ui_can_be_disabled_by_a_view() {
        let custom = parse_view(
            "apiVersion: webui/v1\nkind: WebUIView\nbinding:\n  agent: assistant\nmetadata:\n  name: custom\n  title: Custom\napproval:\n  enabled: false\n",
        )
        .unwrap();
        let view = resolve_view(custom, "assistant").unwrap();
        assert_eq!(view.pointer("/approval/enabled"), Some(&Value::Bool(false)));
    }

    #[test]
    fn mismatched_view_binding_is_rejected() {
        let custom = parse_view(
            "apiVersion: webui/v1\nkind: WebUIView\nbinding:\n  agent: other\nmetadata:\n  name: custom\n  title: Custom\n",
        )
        .unwrap();
        let error = resolve_view(custom, "assistant").unwrap_err();
        assert!(error.to_string().contains("bound to other"));
    }
}
