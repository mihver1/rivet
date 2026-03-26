//! Workflow model: YAML-defined sequences of SSH operations.
//!
//! A workflow describes a series of steps (exec, upload, download) that
//! can be stored in the vault and executed against connections or groups.
//!
//! ## Example YAML
//! ```yaml
//! name: deploy-app
//! description: Deploy application to servers
//! variables:
//!   app_version: "1.0.0"
//!   deploy_path: /opt/app
//! steps:
//!   - name: Stop service
//!     exec:
//!       command: "systemctl stop myapp"
//!   - name: Upload package
//!     upload:
//!       local_path: "./build/app-{{app_version}}.tar.gz"
//!       remote_path: "{{deploy_path}}/app.tar.gz"
//!   - name: Start service
//!     exec:
//!       command: "systemctl start myapp"
//!     on_failure: continue
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A workflow is a named, reusable sequence of steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    /// Unique identifier (assigned on save to vault).
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,

    /// Human-readable name.
    pub name: String,

    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default variable values. Can be overridden at execution time.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub variables: HashMap<String, String>,

    /// Ordered list of steps.
    pub steps: Vec<WorkflowStep>,
}

/// A single step in a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step name (for logging/progress).
    pub name: String,

    /// The action to perform.
    #[serde(flatten)]
    pub action: StepAction,

    /// What to do if this step fails (default: abort).
    #[serde(default, skip_serializing_if = "OnFailure::is_default")]
    pub on_failure: OnFailure,

    /// Optional condition: step runs only if this command exits 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

/// The concrete action of a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepAction {
    /// Execute a shell command on the remote host.
    Exec(ExecAction),

    /// Upload a file to the remote host.
    Upload(TransferAction),

    /// Download a file from the remote host.
    Download(TransferAction),
}

/// Parameters for an exec step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecAction {
    /// The shell command to run. Supports `{{variable}}` templates.
    pub command: String,
}

/// Parameters for an upload or download step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferAction {
    /// Local file path. Supports `{{variable}}` templates.
    pub local_path: String,

    /// Remote file path. Supports `{{variable}}` templates.
    pub remote_path: String,
}

/// Behavior when a step fails.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnFailure {
    /// Stop the workflow immediately.
    #[default]
    Abort,

    /// Log the error and continue to the next step.
    Continue,

    /// Skip the rest of this step (same as continue, but semantic).
    Skip,
}

impl OnFailure {
    fn is_default(&self) -> bool {
        *self == OnFailure::Abort
    }
}

// --- Template expansion ---

/// Expand `{{variable}}` placeholders in a string.
pub fn expand_template(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{key}}}}}");
        result = result.replace(&placeholder, value);
    }
    result
}

impl WorkflowStep {
    /// Return a copy of this step with all template variables expanded.
    pub fn expand(&self, vars: &HashMap<String, String>) -> Self {
        let action = match &self.action {
            StepAction::Exec(e) => StepAction::Exec(ExecAction {
                command: expand_template(&e.command, vars),
            }),
            StepAction::Upload(t) => StepAction::Upload(TransferAction {
                local_path: expand_template(&t.local_path, vars),
                remote_path: expand_template(&t.remote_path, vars),
            }),
            StepAction::Download(t) => StepAction::Download(TransferAction {
                local_path: expand_template(&t.local_path, vars),
                remote_path: expand_template(&t.remote_path, vars),
            }),
        };

        let condition = self
            .condition
            .as_ref()
            .map(|c| expand_template(c, vars));

        WorkflowStep {
            name: expand_template(&self.name, vars),
            action,
            on_failure: self.on_failure.clone(),
            condition,
        }
    }
}

impl Workflow {
    /// Merge runtime variable overrides with the workflow's default variables.
    /// Runtime overrides take precedence.
    pub fn merged_variables(&self, overrides: &HashMap<String, String>) -> HashMap<String, String> {
        let mut vars = self.variables.clone();
        for (k, v) in overrides {
            vars.insert(k.clone(), v.clone());
        }
        vars
    }

    /// Validate the workflow definition.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.name.is_empty() {
            errors.push("workflow name cannot be empty".into());
        }

        if self.steps.is_empty() {
            errors.push("workflow must have at least one step".into());
        }

        for (i, step) in self.steps.iter().enumerate() {
            if step.name.is_empty() {
                errors.push(format!("step {} has no name", i + 1));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// --- YAML parsing ---

/// Parse a workflow from YAML text.
///
/// Uses serde_json as intermediate format since we don't want to add
/// serde_yaml as a dependency to shelly-core. The YAML is parsed using
/// a minimal hand-written parser that covers the workflow subset.
///
/// For full YAML support, use the `parse_workflow_yaml()` function from
/// the shelly-cli or shelly-daemon crate.
pub fn parse_workflow_json(json: &str) -> Result<Workflow, String> {
    serde_json::from_str(json).map_err(|e| format!("invalid workflow JSON: {e}"))
}

/// Serialize a workflow to JSON.
pub fn workflow_to_json(workflow: &Workflow) -> Result<String, String> {
    serde_json::to_string_pretty(workflow).map_err(|e| format!("serialization error: {e}"))
}

// --- Execution result types ---

/// Result of executing an entire workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    pub workflow_name: String,
    pub connection_name: String,
    pub steps: Vec<StepResult>,
    pub success: bool,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub failed_steps: usize,
}

/// Result of a single step execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_name: String,
    pub success: bool,
    pub skipped: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_transferred: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_workflow_json() -> &'static str {
        r#"{
            "name": "deploy-app",
            "description": "Deploy application to servers",
            "variables": {
                "app_version": "1.0.0",
                "deploy_path": "/opt/app"
            },
            "steps": [
                {
                    "name": "Stop service",
                    "exec": {
                        "command": "systemctl stop myapp"
                    }
                },
                {
                    "name": "Upload package",
                    "upload": {
                        "local_path": "./build/app-{{app_version}}.tar.gz",
                        "remote_path": "{{deploy_path}}/app.tar.gz"
                    }
                },
                {
                    "name": "Start service",
                    "exec": {
                        "command": "systemctl start myapp"
                    },
                    "on_failure": "continue"
                },
                {
                    "name": "Verify health",
                    "exec": {
                        "command": "curl -sf http://localhost:8080/health"
                    },
                    "condition": "which curl"
                }
            ]
        }"#
    }

    #[test]
    fn test_parse_workflow() {
        let wf = parse_workflow_json(sample_workflow_json()).unwrap();
        assert_eq!(wf.name, "deploy-app");
        assert_eq!(wf.description.as_deref(), Some("Deploy application to servers"));
        assert_eq!(wf.steps.len(), 4);
        assert_eq!(wf.variables.get("app_version").unwrap(), "1.0.0");
    }

    #[test]
    fn test_step_actions() {
        let wf = parse_workflow_json(sample_workflow_json()).unwrap();

        match &wf.steps[0].action {
            StepAction::Exec(e) => assert_eq!(e.command, "systemctl stop myapp"),
            _ => panic!("expected exec"),
        }

        match &wf.steps[1].action {
            StepAction::Upload(t) => {
                assert_eq!(t.local_path, "./build/app-{{app_version}}.tar.gz");
                assert_eq!(t.remote_path, "{{deploy_path}}/app.tar.gz");
            }
            _ => panic!("expected upload"),
        }
    }

    #[test]
    fn test_on_failure() {
        let wf = parse_workflow_json(sample_workflow_json()).unwrap();
        assert_eq!(wf.steps[0].on_failure, OnFailure::Abort); // default
        assert_eq!(wf.steps[2].on_failure, OnFailure::Continue);
    }

    #[test]
    fn test_condition() {
        let wf = parse_workflow_json(sample_workflow_json()).unwrap();
        assert!(wf.steps[0].condition.is_none());
        assert_eq!(wf.steps[3].condition.as_deref(), Some("which curl"));
    }

    #[test]
    fn test_expand_template() {
        let mut vars = HashMap::new();
        vars.insert("version".into(), "2.0".into());
        vars.insert("path".into(), "/opt".into());

        assert_eq!(expand_template("app-{{version}}", &vars), "app-2.0");
        assert_eq!(expand_template("{{path}}/bin", &vars), "/opt/bin");
        assert_eq!(expand_template("no vars here", &vars), "no vars here");
        assert_eq!(expand_template("{{unknown}}", &vars), "{{unknown}}");
    }

    #[test]
    fn test_step_expand() {
        let step = WorkflowStep {
            name: "Deploy {{version}}".into(),
            action: StepAction::Upload(TransferAction {
                local_path: "./app-{{version}}.tar.gz".into(),
                remote_path: "{{deploy_path}}/app.tar.gz".into(),
            }),
            on_failure: OnFailure::Abort,
            condition: Some("test -d {{deploy_path}}".into()),
        };

        let mut vars = HashMap::new();
        vars.insert("version".into(), "3.0".into());
        vars.insert("deploy_path".into(), "/opt/app".into());

        let expanded = step.expand(&vars);
        assert_eq!(expanded.name, "Deploy 3.0");
        assert_eq!(expanded.condition.as_deref(), Some("test -d /opt/app"));

        match &expanded.action {
            StepAction::Upload(t) => {
                assert_eq!(t.local_path, "./app-3.0.tar.gz");
                assert_eq!(t.remote_path, "/opt/app/app.tar.gz");
            }
            _ => panic!("expected upload"),
        }
    }

    #[test]
    fn test_merged_variables() {
        let wf = parse_workflow_json(sample_workflow_json()).unwrap();

        let mut overrides = HashMap::new();
        overrides.insert("app_version".into(), "2.0.0".into());
        overrides.insert("extra".into(), "value".into());

        let merged = wf.merged_variables(&overrides);
        assert_eq!(merged.get("app_version").unwrap(), "2.0.0"); // overridden
        assert_eq!(merged.get("deploy_path").unwrap(), "/opt/app"); // kept
        assert_eq!(merged.get("extra").unwrap(), "value"); // new
    }

    #[test]
    fn test_validate_ok() {
        let wf = parse_workflow_json(sample_workflow_json()).unwrap();
        assert!(wf.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_name() {
        let json = r#"{
            "name": "",
            "steps": [{ "name": "test", "exec": { "command": "echo ok" } }]
        }"#;
        let wf = parse_workflow_json(json).unwrap();
        let errs = wf.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("name cannot be empty")));
    }

    #[test]
    fn test_validate_no_steps() {
        let json = r#"{ "name": "empty", "steps": [] }"#;
        let wf = parse_workflow_json(json).unwrap();
        let errs = wf.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("at least one step")));
    }

    #[test]
    fn test_workflow_roundtrip() {
        let wf = parse_workflow_json(sample_workflow_json()).unwrap();
        let json = workflow_to_json(&wf).unwrap();
        let wf2 = parse_workflow_json(&json).unwrap();
        assert_eq!(wf.name, wf2.name);
        assert_eq!(wf.steps.len(), wf2.steps.len());
    }

    #[test]
    fn test_step_result() {
        let result = StepResult {
            step_name: "test".into(),
            success: true,
            skipped: false,
            stdout: Some("hello\n".into()),
            stderr: None,
            exit_code: Some(0),
            bytes_transferred: None,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: StepResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.step_name, "test");
        assert!(parsed.success);
    }

    #[test]
    fn test_workflow_result() {
        let result = WorkflowResult {
            workflow_name: "deploy".into(),
            connection_name: "server-1".into(),
            steps: vec![],
            success: true,
            total_steps: 3,
            completed_steps: 3,
            failed_steps: 0,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: WorkflowResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.total_steps, 3);
    }

    #[test]
    fn test_download_action() {
        let json = r#"{
            "name": "fetch-logs",
            "steps": [{
                "name": "Get logs",
                "download": {
                    "remote_path": "/var/log/app.log",
                    "local_path": "./logs/app.log"
                }
            }]
        }"#;
        let wf = parse_workflow_json(json).unwrap();
        match &wf.steps[0].action {
            StepAction::Download(t) => {
                assert_eq!(t.remote_path, "/var/log/app.log");
                assert_eq!(t.local_path, "./logs/app.log");
            }
            _ => panic!("expected download"),
        }
    }
}
