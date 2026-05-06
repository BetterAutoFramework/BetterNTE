//! Pipeline dump/check helpers for CLI and CI usage.

use std::collections::{BTreeSet, VecDeque};
use std::path::Path;

use anyhow::Context;
use serde::Serialize;

use crate::parser::FlowParser;
use crate::types::{Flow, Group, StepKind};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineSourceKind {
    Flow,
    Group,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineStepDump {
    pub id: String,
    pub kind: String,
    pub transition_targets: Vec<String>,
    pub on_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineDump {
    pub source_kind: PipelineSourceKind,
    pub id: String,
    pub name: String,
    pub entry: String,
    pub step_count: usize,
    pub steps: Vec<PipelineStepDump>,
    pub reachable_steps: Vec<String>,
    pub unreachable_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineCheckReport {
    pub ok: bool,
    pub source_kind: PipelineSourceKind,
    pub flow_id: Option<String>,
    pub flow_name: Option<String>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

fn detect_source_kind(json: &str) -> PipelineSourceKind {
    if serde_json::from_str::<Flow>(json).is_ok() {
        PipelineSourceKind::Flow
    } else if serde_json::from_str::<Group>(json).is_ok() {
        PipelineSourceKind::Group
    } else {
        PipelineSourceKind::Flow
    }
}

fn compute_reachability(flow: &Flow) -> (Vec<String>, Vec<String>) {
    let mut visited = BTreeSet::<String>::new();
    if !flow.steps.contains_key(&flow.entry) {
        return (
            Vec::new(),
            flow.steps
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        );
    }
    let mut queue = VecDeque::new();
    queue.push_back(flow.entry.clone());

    while let Some(step_id) = queue.pop_front() {
        if !visited.insert(step_id.clone()) {
            continue;
        }
        if let Some(step) = flow.steps.get(&step_id) {
            for trans in &step.transitions {
                if flow.steps.contains_key(&trans.target) && !visited.contains(&trans.target) {
                    queue.push_back(trans.target.clone());
                }
            }
            if let Some(on_error) = &step.on_error {
                if flow.steps.contains_key(on_error) && !visited.contains(on_error) {
                    queue.push_back(on_error.clone());
                }
            }
        }
    }

    let mut reachable = visited.into_iter().collect::<Vec<_>>();
    let mut unreachable = flow
        .steps
        .keys()
        .filter(|k| !reachable.iter().any(|r| r == *k))
        .cloned()
        .collect::<Vec<_>>();
    reachable.sort();
    unreachable.sort();
    (reachable, unreachable)
}

pub fn dump_pipeline_file(path: &Path) -> anyhow::Result<PipelineDump> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let source_kind = detect_source_kind(&raw);
    let parser = FlowParser::new();
    let flow = parser
        .parse_or_convert_to_flow(&raw)
        .with_context(|| format!("parse {}", path.display()))?;
    let (reachable_steps, unreachable_steps) = compute_reachability(&flow);

    let mut steps = flow
        .steps
        .iter()
        .map(|(id, step)| PipelineStepDump {
            id: id.clone(),
            kind: step_kind_name(&step.kind).to_string(),
            transition_targets: step.transitions.iter().map(|t| t.target.clone()).collect(),
            on_error: step.on_error.clone(),
        })
        .collect::<Vec<_>>();
    steps.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(PipelineDump {
        source_kind,
        id: flow.id,
        name: flow.name,
        entry: flow.entry,
        step_count: flow.steps.len(),
        steps,
        reachable_steps,
        unreachable_steps,
    })
}

pub fn check_pipeline_file(path: &Path) -> PipelineCheckReport {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return PipelineCheckReport {
                ok: false,
                source_kind: PipelineSourceKind::Flow,
                flow_id: None,
                flow_name: None,
                errors: vec![format!("read {} failed: {}", path.display(), e)],
                warnings: Vec::new(),
            }
        }
    };
    let source_kind = detect_source_kind(&raw);
    let parser = FlowParser::new();
    match parser.parse_or_convert_to_flow(&raw) {
        Ok(flow) => {
            let (_reachable, unreachable) = compute_reachability(&flow);
            let mut warnings = Vec::new();
            if !unreachable.is_empty() {
                warnings.push(format!(
                    "{} unreachable steps: {}",
                    unreachable.len(),
                    unreachable.join(", ")
                ));
            }
            PipelineCheckReport {
                ok: true,
                source_kind,
                flow_id: Some(flow.id),
                flow_name: Some(flow.name),
                errors: Vec::new(),
                warnings,
            }
        }
        Err(e) => PipelineCheckReport {
            ok: false,
            source_kind,
            flow_id: None,
            flow_name: None,
            errors: vec![e.to_string()],
            warnings: Vec::new(),
        },
    }
}

fn step_kind_name(kind: &StepKind) -> &'static str {
    kind.type_name()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_temp_json(prefix: &str, content: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join("betternte-runtime-tests");
        std::fs::create_dir_all(&dir).expect("mkdir temp");
        let path = dir.join(format!("{prefix}-{pid}-{ts}.json"));
        std::fs::write(&path, content).expect("write temp json");
        path
    }

    #[test]
    fn dump_reports_unreachable_steps() {
        let flow_json = r#"{
            "id": "f1",
            "name": "flow1",
            "entry": "start",
            "steps": {
                "start": {
                    "kind": { "type": "none" },
                    "transitions": [{ "target": "end", "condition": { "type": "always" } }]
                },
                "end": {
                    "kind": { "type": "none" },
                    "transitions": []
                },
                "dead": {
                    "kind": { "type": "none" },
                    "transitions": []
                }
            }
        }"#;
        let path = write_temp_json("pipeline-flow", flow_json);
        let dump = dump_pipeline_file(&path).expect("dump");
        assert_eq!(dump.id, "f1");
        assert_eq!(dump.step_count, 3);
        assert_eq!(
            dump.reachable_steps,
            vec!["end".to_string(), "start".to_string()]
        );
        assert_eq!(dump.unreachable_steps, vec!["dead".to_string()]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn check_returns_error_for_invalid_pipeline() {
        let invalid = r#"{"not":"a flow or group"}"#;
        let path = write_temp_json("pipeline-invalid", invalid);
        let report = check_pipeline_file(&path);
        assert!(!report.ok);
        assert!(!report.errors.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dump_group_source_kind() {
        let group_json = r#"{
            "name": "group-demo",
            "steps": [
                { "type": "none", "alias": "s1" },
                { "type": "wait", "ms": 100, "alias": "s2" }
            ]
        }"#;
        let path = write_temp_json("pipeline-group", group_json);
        let dump = dump_pipeline_file(&path).expect("dump group");
        assert!(matches!(dump.source_kind, PipelineSourceKind::Group));
        assert_eq!(dump.step_count, 2);
        let _ = std::fs::remove_file(path);
    }
}
