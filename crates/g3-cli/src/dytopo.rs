//! CLI handler for DyTopo mode.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use g3_config::Config;
use g3_ensembles::dytopo::{DyTopoConfig, DyTopoCoordinator};
use g3_index::embeddings::OpenRouterEmbeddings;

use crate::embedded_agents::load_all_agent_files;

pub async fn run_dytopo_mode(
    task: &str,
    config: Config,
    workspace_dir: &std::path::Path,
) -> Result<()> {
    let dytopo_toml = &config.ensembles.dytopo;

    let worker_names: Vec<String> = if dytopo_toml.workers.is_empty() {
        let preferred = ["carmack", "hopper", "euler"];
        let agents = load_all_agent_files(workspace_dir);
        let available_ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
        let mut names: Vec<String> = preferred.iter()
            .filter(|p| available_ids.contains(&p.to_string()))
            .map(|p| p.to_string())
            .collect();
        if names.is_empty() {
            let mut sorted = available_ids;
            sorted.sort();
            sorted.truncate(3);
            names = sorted;
        }
        if names.is_empty() {
            anyhow::bail!("No agents available for DyTopo mode.");
        }
        names
    } else {
        dytopo_toml.workers.clone()
    };

    let all_agents = load_all_agent_files(workspace_dir);
    let mut agent_prompts: HashMap<String, String> = HashMap::new();
    for name in &worker_names {
        let prompt = all_agents.iter()
            .find(|a| a.id == *name)
            .map(|a| a.prompt.clone())
            .unwrap_or_else(|| format!("You are a software engineering agent named '{}'.", name));
        agent_prompts.insert(name.clone(), prompt);
    }

    let dytopo_config = DyTopoConfig {
        max_rounds: dytopo_toml.max_rounds,
        tau_edge: dytopo_toml.tau_edge,
        k_in: dytopo_toml.k_in,
        agent_ids: worker_names.clone(),
        agent_prompts,
    };

    let api_key = resolve_embedding_api_key(&config)?;
    let embedder = Arc::new(OpenRouterEmbeddings::new(
        api_key,
        Some(config.index.embeddings.model.clone()),
        Some(config.index.embeddings.dimensions),
    ));

    println!();
    println!("DyTopo: Dynamic Topology Multi-Agent Collaboration");
    println!("  Workers: {:?}", worker_names);
    println!("  Max rounds: {}, tau: {}, k_in: {}",
        dytopo_config.max_rounds, dytopo_config.tau_edge, dytopo_config.k_in);
    println!("  Embedding: {}", config.index.embeddings.model);
    println!();

    let mut coordinator = DyTopoCoordinator::new(
        dytopo_config, config.clone(), embedder,
    ).await?;

    let result = coordinator.run(task).await?;
    println!("\n=== Final Result ===");
    println!("{}", result);
    Ok(())
}

fn resolve_embedding_api_key(config: &Config) -> Result<String> {
    if let Some(key) = &config.index.embeddings.api_key {
        if !key.is_empty() { return Ok(key.clone()); }
    }
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        if !key.is_empty() { return Ok(key); }
    }
    for (name, provider) in &config.providers.openai_compatible {
        if name.contains("openrouter") || provider.base_url.as_deref().unwrap_or("").contains("openrouter") {
            return Ok(provider.api_key.clone());
        }
    }
    anyhow::bail!("No embedding API key found. Set OPENROUTER_API_KEY or configure [index.embeddings].api_key")
}
