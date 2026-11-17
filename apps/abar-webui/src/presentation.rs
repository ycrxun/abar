use serde_json::Value;

const STRUCTURED_CONFIRMATION: &str = "[structured confirmation]\n";

pub(crate) fn visible_message_input(input: &str) -> String {
    let Some(body) = input.strip_prefix(STRUCTURED_CONFIRMATION) else {
        return input.to_owned();
    };
    let (metadata, template) = body.split_once('\n').unwrap_or((body, ""));
    let metadata = serde_json::from_str::<Value>(metadata).ok();
    let template = template.trim();
    if metadata.is_some() && !template.is_empty() {
        return template.to_owned();
    }
    "确认执行业务方案".to_owned()
}

#[cfg(test)]
mod tests {
    use super::visible_message_input;

    #[test]
    fn preserves_normal_user_input() {
        assert_eq!(visible_message_input("你好"), "你好");
    }

    #[test]
    fn presents_structured_confirmation_as_business_text() {
        let input = concat!(
            "[structured confirmation]\n",
            "{\"kind\":\"example\",\"digest\":\"sha256:abc\",\"snapshot\":\"snapshot-1\"}\n",
            "确认执行当前会话中已展示的业务方案。"
        );
        assert_eq!(
            visible_message_input(input),
            "确认执行当前会话中已展示的业务方案。"
        );
    }

    #[test]
    fn falls_back_to_generic_text_when_template_is_missing() {
        let input = "[structured confirmation]\n{\"kind\":\"example\"}";
        assert_eq!(visible_message_input(input), "确认执行业务方案");
    }
}
