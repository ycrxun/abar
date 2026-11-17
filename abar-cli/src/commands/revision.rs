use crate::support::default_revision_store;
use abar_api::revision::ReviewDecision;
use abar_core::revision::{
    PublishOutcome, RevisionStore, publish_agent_revision, review_agent_revision,
};
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) fn run_publish(arguments: &[String]) -> ExitCode {
    let mut source = PathBuf::from(".");
    let mut json = false;
    let mut positional_seen = false;
    for argument in arguments {
        match argument.as_str() {
            "--json" => json = true,
            value if value.starts_with('-') => {
                eprintln!("unknown option: {value}");
                return ExitCode::from(2);
            }
            value if !positional_seen => {
                source = PathBuf::from(value);
                positional_seen = true;
            }
            value => {
                eprintln!("unexpected argument: {value}");
                return ExitCode::from(2);
            }
        }
    }
    let store = RevisionStore::new(default_revision_store());
    match publish_agent_revision(&store, &source) {
        Ok(outcome) => {
            if json {
                print_publish_json(&outcome);
            } else {
                print_publish(&outcome);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("publish failed: {error}");
            ExitCode::FAILURE
        }
    }
}

pub(crate) fn run_revision_command(arguments: &[String]) -> ExitCode {
    let Some(command) = arguments.first().map(String::as_str) else {
        eprintln!("usage: abar revision list|current|show|approve|reject");
        return ExitCode::from(2);
    };
    let store = RevisionStore::new(default_revision_store());
    match command {
        "list" => list_revisions(&store, &arguments[1..]),
        "current" => current_revision(&store, &arguments[1..]),
        "show" => show_revision(&store, &arguments[1..]),
        "approve" => review_revision(&store, ReviewDecision::Approved, &arguments[1..]),
        "reject" => review_revision(&store, ReviewDecision::Rejected, &arguments[1..]),
        _ => {
            eprintln!("unknown revision command: {command}");
            ExitCode::from(2)
        }
    }
}

fn list_revisions(store: &RevisionStore, arguments: &[String]) -> ExitCode {
    let Some(agent) = arguments.first() else {
        eprintln!("usage: abar revision list <agent>");
        return ExitCode::from(2);
    };
    if arguments.len() != 1 {
        eprintln!("usage: abar revision list <agent>");
        return ExitCode::from(2);
    }
    match store.revisions(agent) {
        Ok(revisions) => {
            for revision in revisions {
                println!("{agent}:{revision}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("revision list failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn current_revision(store: &RevisionStore, arguments: &[String]) -> ExitCode {
    let Some(agent) = arguments.first() else {
        eprintln!("usage: abar revision current <agent>");
        return ExitCode::from(2);
    };
    if arguments.len() != 1 {
        eprintln!("usage: abar revision current <agent>");
        return ExitCode::from(2);
    }
    match store.current(agent) {
        Ok(revision) => {
            print_revision(&revision);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("revision current failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn show_revision(store: &RevisionStore, arguments: &[String]) -> ExitCode {
    let Some(reference) = arguments.first() else {
        eprintln!("usage: abar revision show <agent:revision>");
        return ExitCode::from(2);
    };
    if arguments.len() != 1 {
        eprintln!("usage: abar revision show <agent:revision>");
        return ExitCode::from(2);
    }
    let (agent, revision) = match parse_revision_reference(reference) {
        Ok(value) => value,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match store.get(agent, revision) {
        Ok(revision) => {
            print_revision(&revision);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("revision show failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn review_revision(
    store: &RevisionStore,
    decision: ReviewDecision,
    arguments: &[String],
) -> ExitCode {
    let Some(reference) = arguments.first() else {
        eprintln!("usage: abar revision approve|reject <agent:revision> --reviewer <name>");
        return ExitCode::from(2);
    };
    let (agent, revision) = match parse_revision_reference(reference) {
        Ok(value) => value,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let mut reviewer = None;
    let mut comment = None;
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--reviewer" => {
                index += 1;
                reviewer = arguments.get(index).cloned();
            }
            "--comment" => {
                index += 1;
                comment = arguments.get(index).cloned();
            }
            value => {
                eprintln!("unexpected argument: {value}");
                return ExitCode::from(2);
            }
        }
        index += 1;
    }
    let Some(reviewer) = reviewer else {
        eprintln!("--reviewer is required");
        return ExitCode::from(2);
    };
    match review_agent_revision(store, agent, revision, &reviewer, decision, comment) {
        Ok(evidence) => {
            println!("agent: {}", evidence.agent);
            println!("candidateRevision: {}", evidence.candidate_revision);
            println!("decision: {:?}", evidence.decision);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("scope review failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_revision_reference(reference: &str) -> Result<(&str, u64), String> {
    let Some((agent, revision)) = reference.rsplit_once(':') else {
        return Err("revision reference must be <agent>:<revision>".to_owned());
    };
    let revision = revision
        .parse::<u64>()
        .map_err(|_| format!("invalid revision number: {revision}"))?;
    Ok((agent, revision))
}

fn print_revision(revision: &abar_api::revision::AgentRevision) {
    match serde_json::to_string_pretty(revision) {
        Ok(value) => println!("{value}"),
        Err(error) => eprintln!("cannot encode revision: {error}"),
    }
}

fn print_publish(outcome: &PublishOutcome) {
    let revision = &outcome.revision;
    println!("agent: {}", revision.metadata.agent);
    println!("revision: {}", revision.spec.revision);
    println!("created: {}", outcome.created);
    println!(
        "status: {}",
        if outcome.active {
            "current"
        } else {
            "waiting_review"
        }
    );
    if let Some(diff) = &outcome.scope_diff {
        println!("scopeReviewRequired: {}", diff.review_required);
        for expanded in &diff.expanded {
            println!("expanded: {expanded}");
        }
        for narrowed in &diff.narrowed {
            println!("narrowed: {narrowed}");
        }
    }
}

fn print_publish_json(outcome: &PublishOutcome) {
    let value = serde_json::json!({
        "agent": outcome.revision.metadata.agent,
        "revision": outcome.revision.spec.revision,
        "created": outcome.created,
        "status": if outcome.active { "current" } else { "waiting_review" },
        "scopeReviewRequired": outcome.review_required,
        "scopeDiff": outcome.scope_diff,
    });
    match serde_json::to_string_pretty(&value) {
        Ok(value) => println!("{value}"),
        Err(error) => eprintln!("cannot encode publish result: {error}"),
    }
}
