pub mod client;
pub mod local;

use clap::{Args, Parser, Subcommand};
use client::GatewayClient;
use local::LocalEngine;
use std::io::{self, Write};

#[derive(Parser)]
#[command(
    name = "contextra",
    author = "Soumya Surana",
    version = "0.1.0",
    about = "Contextra CLI - Production-Grade Context Engineering Platform for AI Applications",
    long_about = None
)]
pub struct Cli {
    /// Gateway REST API endpoint URL
    #[arg(
        long,
        global = true,
        env = "CONTEXTRA_GATEWAY_URL",
        default_value = "http://127.0.0.1:3000"
    )]
    pub gateway_url: String,

    /// Optional API key / Auth token for Gateway requests
    #[arg(long, global = true, env = "CONTEXTRA_AUTH_TOKEN")]
    pub auth_token: Option<String>,

    /// Run in local/offline mode directly using local libraries without network calls
    #[arg(long, global = true, env = "CONTEXTRA_LOCAL")]
    pub local: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Ingest a document file into Contextra
    Ingest(IngestArgs),
    /// Chat with Contextra context engine (interactive REPL or single prompt)
    Chat(ChatArgs),
    /// Run evaluation benchmarks against datasets
    Eval(EvalArgs),
    /// Manage document and vector collections
    Collections(CollectionsArgs),
}

#[derive(Args)]
pub struct IngestArgs {
    /// Path to file to ingest
    pub path: String,

    /// Target collection ID or name
    #[arg(long, short)]
    pub collection: Option<String>,
}

#[derive(Args)]
pub struct ChatArgs {
    /// User message. If omitted, opens an interactive chat REPL
    pub message: Option<String>,

    /// Conversation ID to resume
    #[arg(long, short = 'c')]
    pub conversation_id: Option<String>,
}

#[derive(Args)]
pub struct EvalArgs {
    #[command(subcommand)]
    pub command: EvalSubcommands,
}

#[derive(Subcommand)]
pub enum EvalSubcommands {
    /// Run benchmark dataset evaluation
    Run(EvalRunArgs),
}

#[derive(Args)]
pub struct EvalRunArgs {
    /// Path to benchmark dataset JSON file
    #[arg(long, short)]
    pub dataset: Option<String>,

    /// k parameter for retrieval evaluation metrics
    #[arg(long, default_value_t = 3)]
    pub k: usize,
}

#[derive(Args)]
pub struct CollectionsArgs {
    #[command(subcommand)]
    pub command: CollectionsSubcommands,
}

#[derive(Subcommand)]
pub enum CollectionsSubcommands {
    /// List all collections
    List,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ingest(args) => {
            println!("Ingesting document at: {}", args.path);
            if cli.local {
                let engine = LocalEngine::new();
                match engine.ingest(&args.path).await {
                    Ok(result) => {
                        println!("Ingestion completed successfully (local mode):");
                        println!("{result}");
                    }
                    Err(e) => eprintln!("Error ingesting document (local mode): {e}"),
                }
            } else {
                let client = GatewayClient::new(&cli.gateway_url, cli.auth_token.clone());
                match client.ingest_document(&args.path).await {
                    Ok(doc) => {
                        println!("Ingestion submitted successfully to Gateway:");
                        println!("Document ID:    {}", doc.id);
                        println!("Collection ID:  {}", doc.collection_id);
                    }
                    Err(e) => {
                        eprintln!("REST request failed: {e}");
                        eprintln!("Falling back to local ingestion mode...");
                        let engine = LocalEngine::new();
                        match engine.ingest(&args.path).await {
                            Ok(res) => println!("{res}"),
                            Err(err) => eprintln!("Local ingestion failed: {err}"),
                        }
                    }
                }
            }
        }

        Commands::Chat(args) => {
            if let Some(msg) = args.message {
                // Single message mode
                if cli.local {
                    let engine = LocalEngine::new();
                    match engine.chat(&msg).await {
                        Ok(reply) => println!("Assistant: {reply}"),
                        Err(e) => eprintln!("Local chat error: {e}"),
                    }
                } else {
                    let client = GatewayClient::new(&cli.gateway_url, cli.auth_token.clone());
                    let conv_id = match args.conversation_id {
                        Some(id) => id,
                        None => match client.create_conversation(None).await {
                            Ok(conv) => conv.id,
                            Err(e) => {
                                eprintln!("Failed to create conversation: {e}");
                                return Ok(());
                            }
                        },
                    };

                    match client.chat(&conv_id, &msg).await {
                        Ok(resp) => println!("Assistant: {}", resp.message),
                        Err(e) => eprintln!("Chat REST request failed: {e}"),
                    }
                }
            } else {
                // Interactive REPL session
                println!("=== Contextra Chat REPL ===");
                println!("Type 'exit' or press Ctrl+C to quit.\n");

                let client = if !cli.local {
                    Some(GatewayClient::new(&cli.gateway_url, cli.auth_token.clone()))
                } else {
                    None
                };

                let mut current_conv_id = args.conversation_id;
                if let Some(ref c) = client
                    && current_conv_id.is_none()
                    && let Ok(conv) = c.create_conversation(Some("CLI Session".into())).await
                {
                    current_conv_id = Some(conv.id);
                }

                let engine = LocalEngine::new();
                let stdin = io::stdin();

                loop {
                    print!("contextra> ");
                    io::stdout().flush()?;

                    let mut input = String::new();
                    if stdin.read_line(&mut input)? == 0 {
                        break;
                    }

                    let input = input.trim();
                    if input.is_empty() {
                        continue;
                    }
                    if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                        println!("Goodbye!");
                        break;
                    }

                    if cli.local || client.is_none() {
                        match engine.chat(input).await {
                            Ok(reply) => println!("Assistant: {reply}\n"),
                            Err(e) => eprintln!("Error: {e}\n"),
                        }
                    } else if let (Some(c), Some(conv_id)) = (&client, &current_conv_id) {
                        match c.chat(conv_id, input).await {
                            Ok(resp) => println!("Assistant: {}\n", resp.message),
                            Err(e) => eprintln!("Error: {e}\n"),
                        }
                    }
                }
            }
        }

        Commands::Eval(args) => match args.command {
            EvalSubcommands::Run(run_args) => {
                println!("Running evaluation benchmark (k={})...", run_args.k);
                let engine = LocalEngine::new();
                match engine
                    .run_eval(run_args.dataset.as_deref(), run_args.k)
                    .await
                {
                    Ok(report) => println!("\n{report}"),
                    Err(e) => eprintln!("Evaluation error: {e}"),
                }
            }
        },

        Commands::Collections(args) => match args.command {
            CollectionsSubcommands::List => {
                if cli.local {
                    let engine = LocalEngine::new();
                    match engine.list_collections().await {
                        Ok(cols) => {
                            println!("Collections (local mode):");
                            for (id, name) in cols {
                                println!("  - [{id}] {name}");
                            }
                        }
                        Err(e) => eprintln!("Error listing collections: {e}"),
                    }
                } else {
                    let client = GatewayClient::new(&cli.gateway_url, cli.auth_token.clone());
                    match client.list_collections().await {
                        Ok(cols) => {
                            println!("Collections (REST mode):");
                            if cols.is_empty() {
                                println!("  No collections found.");
                            } else {
                                for col in cols {
                                    println!("  - [{}] {}", col.id, col.name);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("REST request failed: {e}");
                            eprintln!("Falling back to local mode...");
                            let engine = LocalEngine::new();
                            if let Ok(cols) = engine.list_collections().await {
                                for (id, name) in cols {
                                    println!("  - [{id}] {name}");
                                }
                            }
                        }
                    }
                }
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parse_help() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
