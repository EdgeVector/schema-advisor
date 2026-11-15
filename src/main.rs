mod cloud;
mod plan;

use clap::{Parser, Subcommand};
use cloud::{CloudAdvice, Environment};
use schema_service_core::{advice::SchemaAdviceRequest, SchemaServiceState};
use serde_json::{json, Value};
use std::{
    error::Error,
    fs,
    io::{self, Read},
    path::Path,
};

#[derive(Parser)]
#[command(version, about = "Review and plan LastDB app schemas")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Review a LastDB app manifest or a standalone draft.
    Review {
        /// Path to JSON, or - for standard input.
        input: String,
        /// Print structured findings.
        #[arg(long)]
        json: bool,
        /// Send draft definitions to Schema Service for advice.
        #[arg(long)]
        online: bool,
        /// Schema Service environment for online review. Defaults to prod.
        #[arg(long, value_enum, requires = "online")]
        env: Option<Environment>,
    },
    /// Plan keys for a draft and print the declaration and app-side key mapping.
    Plan {
        /// Path to a standalone schema draft, or - for standard input.
        input: String,
        /// Stable, unique string field to use as the primary hash.
        #[arg(long, value_name = "FIELD")]
        id: String,
        /// Name and fields for a query ordered within one hash. Repeat for multiple queries.
        #[arg(long, num_args = 3, value_names = ["NAME", "HASH_FIELD", "RANGE_FIELD"])]
        ordered: Vec<String>,
    },
}

fn read_input(input: &str) -> Result<Value, Box<dyn Error>> {
    let bytes = if input == "-" {
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes)?;
        bytes
    } else {
        fs::read(Path::new(input))?
    };
    Ok(serde_json::from_slice(&bytes)?)
}

fn review(
    input: &Value,
    machine_readable: bool,
    online: Option<Environment>,
) -> Result<(), Box<dyn Error>> {
    let doc = input.as_object().ok_or("expected a JSON object")?;
    let schemas = match doc.get("schemas") {
        Some(value) => value
            .as_array()
            .ok_or("schemas must be an array")?
            .as_slice(),
        None => std::slice::from_ref(input),
    };
    let state = SchemaServiceState::from_builtin()?;
    let app_id = doc.get("app_id").and_then(Value::as_str);
    let mut cloud = online.map(CloudAdvice::new);
    let mut reports = Vec::new();
    for schema in schemas {
        let name = schema
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Draft");
        let (findings, source, cloud_error) =
            match cloud.as_mut().map(|cloud| cloud.findings(app_id, schema)) {
                Some(Ok(findings)) => (findings, "schema_service", None),
                fallback => {
                    let advice = state.schema_advice(&SchemaAdviceRequest {
                        app_id: app_id.map(str::to_owned),
                        schema: schema.clone(),
                    })?;
                    let findings = advice
                        .findings
                        .into_iter()
                        .map(serde_json::to_value)
                        .collect::<Result<_, _>>()?;
                    (
                        findings,
                        "bundled_vocabulary",
                        fallback.and_then(Result::err),
                    )
                }
            };
        if machine_readable {
            let mut report = json!({ "schema": name, "findings": findings });
            if online.is_some() {
                report["source"] = json!(source);
                if let Some(error) = cloud_error {
                    report["cloud_error"] = json!(error);
                }
            }
            reports.push(report);
        } else {
            println!("{name}");
            if let Some(error) = cloud_error {
                println!("  {error} Using the bundled vocabulary.");
            } else if online.is_some() {
                println!("  Advice from Schema Service.");
            }
            if findings.is_empty() {
                println!("  No findings.");
            }
            for message in findings
                .iter()
                .filter_map(|finding| finding["message"].as_str())
            {
                println!("  {message}");
            }
        }
    }
    if machine_readable {
        println!("{}", serde_json::to_string_pretty(&reports)?);
    } else if schemas.is_empty() {
        println!("This app declares no schemas.");
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::Review {
            input,
            json,
            online,
            env,
        } => review(
            &read_input(&input)?,
            json,
            online.then_some(env.unwrap_or_default()),
        ),
        Command::Plan { input, id, ordered } => {
            let proposal = plan::schema(&read_input(&input)?, &id, &ordered)?;
            println!("{}", serde_json::to_string_pretty(&proposal)?);
            Ok(())
        }
    }
}
