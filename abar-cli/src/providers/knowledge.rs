use super::message_config::FileKnowledgeConfig;
use abar_api::context::RequestContext;
use abar_api::error::ErrorCode;
use abar_api::knowledge::{KnowledgeDocument, KnowledgeProvider, KnowledgeQuery};
use abar_api::provider::{BoxFuture, ProviderError};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

#[derive(Clone, Debug)]
pub(super) struct FileKnowledgeProvider {
    sources: BTreeMap<String, FileKnowledgeConfig>,
}

impl FileKnowledgeProvider {
    pub(super) fn new(sources: BTreeMap<String, FileKnowledgeConfig>) -> Self {
        Self { sources }
    }
}

impl KnowledgeProvider for FileKnowledgeProvider {
    fn retrieve<'a>(
        &'a self,
        _context: &'a RequestContext,
        query: &'a KnowledgeQuery,
    ) -> BoxFuture<'a, Result<Vec<KnowledgeDocument>, ProviderError>> {
        let result = self.retrieve_sync(query);
        Box::pin(async move { result })
    }
}

impl FileKnowledgeProvider {
    fn retrieve_sync(
        &self,
        query: &KnowledgeQuery,
    ) -> Result<Vec<KnowledgeDocument>, ProviderError> {
        let source = self.sources.get(&query.source).ok_or_else(|| {
            provider_error(
                ErrorCode::InvalidRequest,
                format!("unknown Knowledge Source: {}", query.source),
            )
        })?;
        let mut documents = Vec::new();
        collect_documents(&source.path, &source.path, &query.source, &mut documents)?;
        let terms = query
            .query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .collect::<Vec<_>>();
        documents.sort_by(|left, right| {
            score(&right.content, &terms)
                .cmp(&score(&left.content, &terms))
                .then_with(|| left.document_id.cmp(&right.document_id))
        });
        documents.truncate(query.top_k as usize);
        Ok(documents)
    }
}

fn collect_documents(
    root: &Path,
    current: &Path,
    source_id: &str,
    documents: &mut Vec<KnowledgeDocument>,
) -> Result<(), ProviderError> {
    for entry in fs::read_dir(current).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let file_type = entry.file_type().map_err(io_error)?;
        if file_type.is_symlink() {
            return Err(provider_error(
                ErrorCode::InvalidRequest,
                "Knowledge Source contains a symlink",
            ));
        }
        if file_type.is_dir() {
            collect_documents(root, &entry.path(), source_id, documents)?;
        } else if file_type.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| matches!(extension.to_str(), Some("md" | "txt")))
        {
            let content = fs::read_to_string(entry.path()).map_err(io_error)?;
            let document_id = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(&entry.path())
                .to_string_lossy()
                .into_owned();
            let digest = abar_core::digest::sha256_hex(content.as_bytes());
            let retrieved_at = OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .map_err(|error| provider_error(ErrorCode::InvalidRequest, error.to_string()))?;
            documents.push(KnowledgeDocument {
                source: source_id.to_owned(),
                document_id,
                version: digest.clone(),
                digest,
                content,
                retrieved_at,
            });
        }
    }
    Ok(())
}

fn score(content: &str, terms: &[&str]) -> usize {
    terms.iter().map(|term| content.matches(term).count()).sum()
}

fn io_error(error: std::io::Error) -> ProviderError {
    provider_error(ErrorCode::InvalidRequest, error.to_string())
}

fn provider_error(code: ErrorCode, message: impl Into<String>) -> ProviderError {
    ProviderError::new(code, message, false)
}

#[cfg(test)]
mod tests {
    use super::FileKnowledgeProvider;
    use crate::providers::message_config::FileKnowledgeConfig;
    use abar_api::conformance::validate_knowledge_response;
    use abar_api::knowledge::KnowledgeQuery;
    use std::collections::BTreeMap;

    #[test]
    fn file_provider_satisfies_knowledge_contract() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("policy.md"),
            "policy and source rules",
        )
        .unwrap();
        let provider = FileKnowledgeProvider::new(BTreeMap::from([(
            "policy".to_owned(),
            FileKnowledgeConfig {
                path: directory.path().to_path_buf(),
            },
        )]));
        let query = KnowledgeQuery {
            source: "policy".to_owned(),
            query: "policy".to_owned(),
            top_k: 3,
        };
        let documents = provider.retrieve_sync(&query).unwrap();
        assert!(
            validate_knowledge_response(&query, &documents).is_empty(),
            "{:?}",
            validate_knowledge_response(&query, &documents)
        );
    }
}
