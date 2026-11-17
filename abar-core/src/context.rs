//! Deterministic model-context assembly for one Message.

use abar_api::knowledge::KnowledgeDocument;
use abar_api::message::Message;
use abar_api::message::MessageState;
use abar_api::provider::{SkillContent, SkillMetadata};
use std::fmt::Write;

/// One already-projected tool result available to the model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextToolResult<'a> {
    pub tool_id: &'a str,
    pub content: &'a str,
}

/// Inputs available to one Message context assembly.
#[derive(Clone, Copy, Debug)]
pub struct ContextInput<'a> {
    pub system_prompt: &'a str,
    pub automatic_skills: &'a [SkillMetadata],
    pub manual_skills: &'a [SkillContent],
    pub knowledge: &'a [KnowledgeDocument],
    pub history: &'a [Message],
    pub tool_results: &'a [ContextToolResult<'a>],
    pub current_input: &'a str,
}

/// Assembles deterministic model input while retaining only completed history.
#[derive(Clone, Copy, Debug)]
pub struct ContextAssembler {
    pub max_history_messages: usize,
}

impl ContextAssembler {
    /// Creates an assembler with the selected history bound.
    #[must_use]
    pub const fn new(max_history_messages: usize) -> Self {
        Self {
            max_history_messages,
        }
    }

    /// Builds the model-visible string for one Message.
    #[must_use]
    pub fn assemble(&self, input: &ContextInput<'_>) -> String {
        let mut output = String::new();
        writeln!(output, "[agent]\n{}", input.system_prompt.trim()).expect("String write");

        if !input.automatic_skills.is_empty() {
            output.push_str("\n[available skills]\n");
            let mut skills = input.automatic_skills.iter().collect::<Vec<_>>();
            skills.sort_by(|left, right| left.name.cmp(&right.name));
            for skill in skills {
                writeln!(output, "- {}: {}", skill.name, skill.description).expect("String write");
            }
        }

        if !input.manual_skills.is_empty() {
            output.push_str("\n[selected skills]\n");
            for skill in input.manual_skills {
                writeln!(
                    output,
                    "## {}\n{}",
                    skill.metadata.name,
                    skill.markdown.trim()
                )
                .expect("String write");
            }
        }

        if !input.knowledge.is_empty() {
            output.push_str("\n[knowledge]\n");
            for document in input.knowledge {
                writeln!(
                    output,
                    "## {}:{}@{}\n{}",
                    document.source,
                    document.document_id,
                    document.version,
                    document.content.trim()
                )
                .expect("String write");
            }
        }

        let history = completed_history(input.history, self.max_history_messages);
        if !history.is_empty() {
            output.push_str("\n[conversation]\n");
            for message in history {
                writeln!(output, "user: {}", message.input.trim()).expect("String write");
                if let Some(value) = &message.output {
                    writeln!(output, "assistant: {}", value.trim()).expect("String write");
                }
            }
        }

        if !input.tool_results.is_empty() {
            output.push_str("\n[tool results]\n");
            for result in input.tool_results {
                writeln!(output, "## {}\n{}", result.tool_id, result.content.trim())
                    .expect("String write");
            }
        }

        writeln!(output, "\n[user]\n{}", input.current_input.trim()).expect("String write");
        output
    }
}

fn completed_history(history: &[Message], limit: usize) -> Vec<&Message> {
    let mut completed = history
        .iter()
        .filter(|message| message.state == MessageState::Completed && message.output.is_some())
        .collect::<Vec<_>>();
    completed.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    if completed.len() > limit {
        completed.drain(..completed.len() - limit);
    }
    completed
}

#[cfg(test)]
mod tests {
    use super::{ContextAssembler, ContextInput, ContextToolResult};
    use abar_api::knowledge::KnowledgeDocument;
    use abar_api::message::{Message, MessageState};
    use abar_api::provider::{SkillContent, SkillMetadata, ToolEffect};
    use abar_api::revision::AgentRevision;

    fn message(id: &str, input: &str, output: &str) -> Message {
        let mut message: Message =
            serde_json::from_str(include_str!("../../abar-api/fixtures/core/message.json"))
                .unwrap();
        message.id = id.to_owned();
        message.state = MessageState::Completed;
        message.input = input.to_owned();
        message.output = Some(output.to_owned());
        message
    }

    #[test]
    fn assembles_only_completed_recent_history_and_citations() {
        let revision: AgentRevision = serde_json::from_str(include_str!(
            "../../abar-api/fixtures/core/agent-revision.json"
        ))
        .unwrap();
        let history = vec![
            message("msg-1", "first", "old answer"),
            message("msg-2", "second", "recent answer"),
            {
                let mut failed = message("msg-3", "failed", "failed answer");
                failed.state = MessageState::Failed;
                failed
            },
        ];
        let skills = [SkillMetadata {
            name: "decision-guidance".to_owned(),
            description: "make decisions".to_owned(),
            disable_model_invocation: false,
            version: Some("11".to_owned()),
        }];
        let manual_skills = [SkillContent {
            metadata: skills[0].clone(),
            markdown: "compare available records".to_owned(),
        }];
        let knowledge = [KnowledgeDocument {
            source: "policy".to_owned(),
            document_id: "doc-12".to_owned(),
            version: "2026-11-17".to_owned(),
            digest: revision.spec.behavior.prompt.digest.clone(),
            content: "protect display stock".to_owned(),
            retrieved_at: "2026-11-17T09:00:00Z".to_owned(),
        }];
        let results = [ContextToolResult {
            tool_id: "catalog.query-example",
            content: "{\"available\": 10}",
        }];
        let assembled = ContextAssembler::new(1).assemble(&ContextInput {
            system_prompt: "system",
            automatic_skills: &skills,
            manual_skills: &manual_skills,
            knowledge: &knowledge,
            history: &history,
            tool_results: &results,
            current_input: "current",
        });
        assert!(assembled.contains("system"));
        assert!(assembled.contains("compare available records"));
        assert!(assembled.contains("policy:doc-12@2026-11-17"));
        assert!(assembled.contains("second"));
        assert!(assembled.contains("recent answer"));
        assert!(!assembled.contains("first"));
        assert!(!assembled.contains("failed answer"));
        assert!(assembled.contains("current"));
    }

    #[test]
    fn tool_effect_type_remains_serializable_for_context_fixtures() {
        assert_eq!(
            serde_json::to_string(&ToolEffect::Read).unwrap(),
            "\"read\""
        );
    }
}
