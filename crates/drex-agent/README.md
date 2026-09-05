# Drex Agent

Planning and execution orchestration for the Drex agent framework.

## Overview

The `drex-agent` crate provides the core agent loop that:

1. **Plans**: Generates natural language plans from user requests
2. **Translates**: Converts plan steps into structured tool calls
3. **Executes**: Runs tools through the ToolRegistry with proper capability checks
4. **Observes**: Evaluates results and decides whether to continue or replan
5. **Remembers**: Stores useful information back to memory

## Modules

- `planner`: Natural language plan generation using ModelRouter

## Usage

```rust
use drex_agent::Planner;
use drex_memory::MemoryStore;
use drex_models::router::ModelRouter;
use std::sync::Arc;

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    let router = Arc::new(ModelRouter::new());
    let planner = Planner::new(router);

    let plan = planner.plan("Find all Rust files", None::<&dyn MemoryStore>).await?;

    println!("Plan has {} steps", plan.step_count());
    Ok(())
}
```
