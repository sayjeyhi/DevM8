#![allow(unused_imports)]

use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

pub mod adf;
pub mod client;
pub mod types;

pub use adf::{adf_to_text, to_adf, AdfNode};
pub use client::{IssuesPage, JiraClient, ProjectInfo, StatusInfo};
pub use types::{JiraClientConfig, JiraIssue};

const ISSUE_KEY_PATTERN: &str = r"\b([A-Za-z][A-Za-z0-9]{1,9})-(\d+)\b";

fn issue_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(ISSUE_KEY_PATTERN).unwrap())
}

/// Scan `text` for Jira issue keys (e.g. "ABC-123") belonging to a known project.
///
/// `project_keys` is the allow-list of configured Jira project keys — without it,
/// free text is too prone to false positives (e.g. "utf-8", "iso-9001") to trust.
/// Returns unique keys, normalized to uppercase, in first-seen order.
pub fn extract_issue_keys(text: &str, project_keys: &[String]) -> Vec<String> {
    if project_keys.is_empty() {
        return Vec::new();
    }

    let known: HashSet<String> = project_keys.iter().map(|k| k.to_uppercase()).collect();

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for cap in issue_key_re().captures_iter(text) {
        let project = cap[1].to_uppercase();
        if !known.contains(&project) {
            continue;
        }
        let key = format!("{}-{}", project, &cap[2]);
        if seen.insert(key.clone()) {
            out.push(key);
        }
    }
    out
}
