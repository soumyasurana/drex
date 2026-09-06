mod health_check;
mod security;
mod state;

use clap::{Parser, Subcommand};
use drex_agent::{Agent, AgentConfig};
use drex_core::{
    MemoryConfig, initialize_app_state,
    health_check::{check_memory, check_postgres, check_redis, HealthStatus},
    security::SecuritySeverity,
};
use drex_config::AppConfig;
use tracing::{error, info};
use std::sync::Arc;

/// Drex - AI Agent System
#[derive(Parser)]
#[command(name = "drex")]
#[command(about = "Drex AI Agent - Your local-first AI assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run health checks on all subsystems
    Health,
    /// Run security audit
    Security,
    /// Ask Drex to perform a task
    Ask {
        /// The request to send to Drex
        request: Vec<String>,

        /// Show execution trace
        #[arg(long)]
        trace: bool,

        /// Don't actually execute, just show what would be done
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Health => run_health_check().await,
        Commands::Security => do_security_audit().await,
        Commands::Ask { request, trace, dry_run } => {
            run_ask(request.join(" "), trace, dry_run).await
        }
    }
}

async fn run_health_check() {
    // Load configuration
    let config = match AppConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    println!("Drex Health Check");
    println!("================");
    println!();

    // Check PostgreSQL
    print!("PostgreSQL: ");
    let postgres_health = check_postgres(&config).await;
    match &postgres_health {
        HealthStatus::Healthy => println!("✓ Healthy"),
        HealthStatus::Unhealthy(msg) => println!("✗ Unhealthy: {}", msg),
    }

    // Check Redis
    print!("Redis:      ");
    let redis_health = check_redis(&config).await;
    match &redis_health {
        HealthStatus::Healthy => println!("✓ Healthy"),
        HealthStatus::Unhealthy(msg) => println!("✗ Unhealthy: {}", msg),
    }

    // Check Memory
    print!("Memory:     ");
    let memory_config = MemoryConfig::default();
    let (app_state, memory_health) = match initialize_app_state(config, memory_config).await {
        Ok(state) => {
            let health = check_memory(Some(&state)).await;
            (Some(state), health)
        }
        Err(e) => {
            let msg = format!("Failed to initialize memory: {}", e);
            (None, HealthStatus::Unhealthy(msg))
        }
    };
    match &memory_health {
        HealthStatus::Healthy => println!("✓ Healthy"),
        HealthStatus::Unhealthy(msg) => println!("✗ Unhealthy: {}", msg),
    }

    println!();

    let all_healthy = matches!(postgres_health, HealthStatus::Healthy)
        && matches!(redis_health, HealthStatus::Healthy)
        && matches!(memory_health, HealthStatus::Healthy);

    if all_healthy {
        println!("All systems are healthy!");
        std::process::exit(0);
    } else {
        println!("Some systems are unhealthy. Check the logs for details.");
        std::process::exit(1);
    }
}

async fn run_ask(request: String, _trace: bool, dry_run: bool) {
    if request.is_empty() {
        eprintln!("Error: request cannot be empty");
        eprintln!("Usage: drex ask '<your request>'");
        std::process::exit(1);
    }

    // Load configuration
    let config = match AppConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level)),
        )
        .init();

    info!(request = %request, "Drex ask command");

    if dry_run {
        println!("[DRY RUN] Would process request: {}", request);
        println!("[DRY RUN] Configuration:");
        println!("  - App name: {}", config.app_name);
        println!("  - Environment: {}", config.environment);
        return;
    }

    // Initialize the memory system
    info!("Initializing memory subsystem...");
    let memory_config = MemoryConfig::default();

    // Create agent with Ollama backend configured
    let mut model_router = drex_models::router::ModelRouter::new();

    // Register Ollama backend from configuration
    let ollama_backend = drex_models::backends::OllamaBackend::from_drex_config(&config.ollama);
    model_router.register(
        drex_models::router::TaskKind::Main,
        Box::new(ollama_backend),
    );

    let model_router = Arc::new(model_router);

    let _app_state = match initialize_app_state(config, memory_config).await {
        Ok(state) => state,
        Err(e) => {
            error!("Failed to initialize: {}", e);
            eprintln!("Error: Failed to initialize Drex: {}", e);
            std::process::exit(1);
        }
    };

    println!("Processing request: {}", request);
    println!();

    // Initialize tool registry and register all available tools
    let mut tool_registry = drex_tools::ToolRegistry::new();
    let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));

    // Register harmless tools (no capabilities required)
    tool_registry.register(Box::new(drex_tools::tools::EchoTool::new()))
        .map_err(|e| eprintln!("Warning: Failed to register echo tool: {}", e)).ok();

    // Register filesystem tool with config restricting to current directory
    let fs_config = drex_tools::tools::FileSystemConfig::new(&current_dir);
    tool_registry.register(Box::new(drex_tools::tools::FileSystemReadTool::new(fs_config)))
        .map_err(|e| eprintln!("Warning: Failed to register filesystem tool: {}", e)).ok();

    // Register terminal tool
    let terminal_config = drex_tools::tools::TerminalConfig::new();
    tool_registry.register(Box::new(drex_tools::tools::TerminalExecuteTool::new(terminal_config)))
        .map_err(|e| eprintln!("Warning: Failed to register terminal tool: {}", e)).ok();

    // Register git tools with config
    let git_config = drex_tools::tools::GitConfig::new(&current_dir);
    tool_registry.register(Box::new(drex_tools::tools::GitStatusTool::new(git_config.clone())))
        .map_err(|e| eprintln!("Warning: Failed to register git_status tool: {}", e)).ok();
    tool_registry.register(Box::new(drex_tools::tools::GitDiffTool::new(git_config)))
        .map_err(|e| eprintln!("Warning: Failed to register git_diff tool: {}", e)).ok();

    // Register web fetch tool
    let web_config = drex_tools::tools::WebFetchConfig::new();
    tool_registry.register(Box::new(drex_tools::tools::WebFetchTool::new(web_config)))
        .map_err(|e| eprintln!("Warning: Failed to register web_fetch tool: {}", e)).ok();

    // Register memory tool for storing/retrieving memories
    tool_registry.register(Box::new(drex_tools::tools::MemoryTool::new()))
        .map_err(|e| eprintln!("Warning: Failed to register memory tool: {}", e)).ok();

    let tool_registry = Arc::new(tool_registry);

    // Check if we have backends registered
    if !model_router.has_route_for(drex_models::router::TaskKind::Main) {
        println!("Error: No model backend is configured.");
        println!();
        println!("To use Drex, you need to configure a model backend.");
        println!("Add configuration to your Drex config file to set up a model provider.");
        std::process::exit(1);
    }

    let capabilities = drex_tools::CapabilitySet::new();
    let agent_config = AgentConfig::default();

    let agent = Agent::new(model_router, tool_registry, capabilities, agent_config);

    // Execute the agent
    match agent.execute(&request, None).await {
        Ok(result) => {
            println!();
            println!("==================");
            println!("Drex's Response:");
            println!("==================");
            println!();
            println!("{}", result.response);
            println!();
            println!("Stats:");
            println!("  - Steps executed: {}", result.steps_executed);
            println!("  - Observations: {}", result.observations.len());
            println!("  - Memories written: {}", result.memories_written);
        }
        Err(e) => {
            eprintln!();
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn do_security_audit() {
    println!("Drex Security Audit");
    println!("==================");
    println!();

    let summary = drex_core::security::run_security_audit().await;

    println!("Audit completed at: {:?}", summary.timestamp);
    println!();

    for (audit_name, result) in &summary.results {
        println!("{}:", audit_name);
        println!("  Status: {}", if result.passed { "PASS" } else { "FAIL" });
        println!("  Findings: {}", result.findings.len());
        if !result.findings.is_empty() {
            println!("  Details:");
            for finding in &result.findings {
                println!("    - [{}] {}: {}",
                    format_severity(finding.severity),
                    finding.category,
                    finding.description
                );
                println!("      Recommendation: {}", finding.recommendation);
            }
        }
        println!();
    }

    println!("Summary:");
    println!("  Total findings: {}", summary.total_findings);

    if summary.critical_count > 0 {
        println!("  Critical: {} ⚠️", summary.critical_count);
    }
    if summary.high_count > 0 {
        println!("  High: {} ⚠️", summary.high_count);
    }

    if summary.all_passed {
        println!("\n✅ All security audits passed!");
        std::process::exit(0);
    } else {
        println!("\n⚠️ Some security audits failed. Review findings above.");
        std::process::exit(1);
    }
}

fn format_severity(severity: SecuritySeverity) -> &'static str {
    match severity {
        SecuritySeverity::Critical => "CRIT",
        SecuritySeverity::High => "HIGH",
        SecuritySeverity::Medium => "MED",
        SecuritySeverity::Low => "LOW",
        SecuritySeverity::Info => "INFO",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_ask_command() {
        let cli = Cli::parse_from(["drex", "ask", "hello", "world"]);
        match cli.command {
            Commands::Ask { request, trace, dry_run } => {
                assert_eq!(request, vec!["hello", "world"]);
                assert!(!trace);
                assert!(!dry_run);
            }
            _ => panic!("Expected Ask command"),
        }
    }

    #[test]
    fn cli_parses_ask_with_trace() {
        let cli = Cli::parse_from(["drex", "ask", "--trace", "hello"]);
        match cli.command {
            Commands::Ask { trace, .. } => {
                assert!(trace);
            }
            _ => panic!("Expected Ask command"),
        }
    }

    #[test]
    fn cli_parses_health_command() {
        let cli = Cli::parse_from(["drex", "health"]);
        match cli.command {
            Commands::Health => {}
            _ => panic!("Expected Health command"),
        }
    }

    #[test]
    fn cli_parses_security_command() {
        let cli = Cli::parse_from(["drex", "security"]);
        match cli.command {
            Commands::Security => {}
            _ => panic!("Expected Security command"),
        }
    }
}
