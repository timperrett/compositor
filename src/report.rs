use crate::model::{CommandReport, Severity, ValidationReport, SCHEMA_VERSION};
use serde::Serialize;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

pub fn render<T: Serialize + std::fmt::Debug>(
    format: OutputFormat,
    command: &str,
    data: T,
    validation: ValidationReport,
) -> Result<String, crate::AppError> {
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(&CommandReport {
            schema_version: SCHEMA_VERSION,
            command: command.into(),
            data,
            validation,
        })
        .map(|value| format!("{value}\n"))
        .map_err(|error| crate::AppError::Serialization(error.to_string())),
        OutputFormat::Human => {
            let mut output = format!("{command}\n{data:#?}\n");
            if !validation.issues.is_empty() {
                output.push_str("Validation:\n");
                for issue in validation.issues {
                    output.push_str(&format!(
                        "- {} [{}] {}\n",
                        severity_label(&issue.severity),
                        issue.code,
                        issue.message
                    ));
                }
            }
            Ok(output)
        }
    }
}

fn severity_label(value: &Severity) -> &'static str {
    match value {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
        Severity::Blocking => "blocking",
    }
}
