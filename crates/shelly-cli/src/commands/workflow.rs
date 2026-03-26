use std::collections::HashMap;

use comfy_table::{presets, Table};
use shelly_core::protocol::*;
use shelly_core::workflow::{Workflow, WorkflowResult};

use super::{CliError, get_client};

pub async fn list() -> Result<(), CliError> {
    let mut client = get_client().await?;

    let result = client
        .call("workflow.list", None)
        .await
        .map_err(CliError::Client)?;

    let workflows: Vec<Workflow> =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    if workflows.is_empty() {
        println!("No workflows. Import one with: shelly workflow import <file.yaml>");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(presets::UTF8_FULL_CONDENSED);
    table.set_header(vec!["Name", "Description", "Steps", "Variables"]);

    for wf in &workflows {
        let vars = if wf.variables.is_empty() {
            String::new()
        } else {
            wf.variables
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        };

        table.add_row(vec![
            &wf.name,
            wf.description.as_deref().unwrap_or(""),
            &wf.steps.len().to_string(),
            &vars,
        ]);
    }

    println!("{table}");
    Ok(())
}

pub async fn show(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("workflow name".into()))?;

    let mut client = get_client().await?;

    let params = serde_json::to_value(WorkflowGetParams {
        id: None,
        name: Some(name.clone()),
    })
    .unwrap();

    let result = client
        .call("workflow.get", Some(params))
        .await
        .map_err(CliError::Client)?;

    let wf: Workflow =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Name:        {}", wf.name);
    println!("ID:          {}", wf.id);
    if let Some(ref desc) = wf.description {
        println!("Description: {desc}");
    }
    if !wf.variables.is_empty() {
        println!("Variables:");
        for (k, v) in &wf.variables {
            println!("  {k} = {v}");
        }
    }

    println!("Steps ({}):", wf.steps.len());
    for (i, step) in wf.steps.iter().enumerate() {
        let action_desc = match &step.action {
            shelly_core::workflow::StepAction::Exec(e) => format!("exec: {}", e.command),
            shelly_core::workflow::StepAction::Upload(t) => {
                format!("upload: {} -> {}", t.local_path, t.remote_path)
            }
            shelly_core::workflow::StepAction::Download(t) => {
                format!("download: {} -> {}", t.remote_path, t.local_path)
            }
        };

        let mut extras = Vec::new();
        if step.on_failure != shelly_core::workflow::OnFailure::Abort {
            extras.push(format!("on_failure: {:?}", step.on_failure));
        }
        if let Some(ref cond) = step.condition {
            extras.push(format!("if: {cond}"));
        }

        let suffix = if extras.is_empty() {
            String::new()
        } else {
            format!(" ({})", extras.join(", "))
        };

        println!("  {}. {} — {action_desc}{suffix}", i + 1, step.name);
    }

    Ok(())
}

pub async fn import(args: &[String]) -> Result<(), CliError> {
    let file_path = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("YAML file path".into()))?;

    let yaml = std::fs::read_to_string(file_path)
        .map_err(|e| CliError::Other(format!("cannot read {file_path}: {e}")))?;

    // Validate YAML locally first
    let wf: Workflow = serde_yaml::from_str(&yaml)
        .map_err(|e| CliError::Other(format!("invalid YAML: {e}")))?;

    if let Err(errs) = wf.validate() {
        return Err(CliError::Other(format!(
            "workflow validation failed: {}",
            errs.join("; ")
        )));
    }

    let mut client = get_client().await?;

    let params = serde_json::to_value(WorkflowImportParams { yaml }).unwrap();

    let result = client
        .call("workflow.import", Some(params))
        .await
        .map_err(CliError::Client)?;

    let id_result: IdResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Workflow '{}' imported (id: {})", wf.name, id_result.id);
    Ok(())
}

pub async fn rm(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("workflow name".into()))?;

    let mut client = get_client().await?;

    let params = serde_json::to_value(WorkflowDeleteParams {
        id: None,
        name: Some(name.clone()),
    })
    .unwrap();

    client
        .call("workflow.delete", Some(params))
        .await
        .map_err(CliError::Client)?;

    println!("Workflow '{name}' deleted.");
    Ok(())
}

pub async fn run(args: &[String]) -> Result<(), CliError> {
    // Usage: shelly workflow run <name> <connection|--group group-name> [--var key=value ...]
    if args.is_empty() {
        return Err(CliError::MissingArgument("workflow name".into()));
    }

    let wf_name = &args[0];
    let mut connection_name = None;
    let mut group_name = None;
    let mut variables = HashMap::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--group" | "-g" => {
                i += 1;
                group_name = Some(
                    args.get(i)
                        .ok_or_else(|| CliError::MissingArgument("group name after --group".into()))?
                        .clone(),
                );
            }
            "--var" | "-v" => {
                i += 1;
                let kv = args
                    .get(i)
                    .ok_or_else(|| CliError::MissingArgument("key=value after --var".into()))?;
                if let Some((k, v)) = kv.split_once('=') {
                    variables.insert(k.to_string(), v.to_string());
                } else {
                    return Err(CliError::Other(format!("invalid --var format: {kv} (expected key=value)")));
                }
            }
            other => {
                if connection_name.is_none() && group_name.is_none() {
                    connection_name = Some(other.to_string());
                } else {
                    return Err(CliError::Other(format!("unexpected argument: {other}")));
                }
            }
        }
        i += 1;
    }

    if connection_name.is_none() && group_name.is_none() {
        return Err(CliError::MissingArgument(
            "target: connection name or --group <name>".into(),
        ));
    }

    let params = serde_json::to_value(WorkflowRunParams {
        workflow_id: None,
        workflow_name: Some(wf_name.clone()),
        connection_id: None,
        connection_name,
        group_id: None,
        group_name,
        variables,
    })
    .unwrap();

    let mut client = get_client().await?;
    let result = client
        .call("workflow.run", Some(params))
        .await
        .map_err(CliError::Client)?;

    let results: Vec<WorkflowResult> =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    for wf_result in &results {
        println!("=== {} on {} ===", wf_result.workflow_name, wf_result.connection_name);

        for step in &wf_result.steps {
            let status = if step.skipped {
                "SKIP"
            } else if step.success {
                "OK"
            } else {
                "FAIL"
            };

            print!("[{status}] {}", step.step_name);

            if let Some(exit_code) = step.exit_code {
                if exit_code != 0 {
                    print!(" (exit: {exit_code})");
                }
            }

            if let Some(bytes) = step.bytes_transferred {
                print!(" ({bytes} bytes)");
            }

            println!();

            if let Some(ref stdout) = step.stdout {
                if !stdout.is_empty() {
                    for line in stdout.lines() {
                        println!("  {line}");
                    }
                }
            }

            if let Some(ref stderr) = step.stderr {
                if !stderr.is_empty() {
                    for line in stderr.lines() {
                        eprintln!("  stderr: {line}");
                    }
                }
            }

            if let Some(ref err) = step.error {
                eprintln!("  error: {err}");
            }
        }

        println!(
            "--- {}/{} steps completed, {} failed{}\n",
            wf_result.completed_steps,
            wf_result.total_steps,
            wf_result.failed_steps,
            if wf_result.success { "" } else { " [FAILED]" }
        );
    }

    // Exit code: 0 if all succeeded, 1 if any failed
    if results.iter().all(|r| r.success) {
        Ok(())
    } else {
        Err(CliError::Other("workflow failed on one or more connections".into()))
    }
}
