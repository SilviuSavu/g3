//! DyTopo coordinator: orchestrates the round loop.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use g3_config::Config;
use g3_index::embeddings::EmbeddingProvider;

use super::descriptor::{DescriptorPair, EmbeddedDescriptors};
use super::manager::ManagerAgent;
use super::message;
use super::topology::TopologyBuilder;
use super::worker::AgentWorker;

#[derive(Debug, Clone)]
pub struct DyTopoConfig {
    pub max_rounds: usize,
    pub tau_edge: f32,
    pub k_in: usize,
    pub agent_ids: Vec<String>,
    pub agent_prompts: HashMap<String, String>,
}

pub struct DyTopoCoordinator {
    config: DyTopoConfig,
    #[allow(dead_code)]
    llm_config: Config,
    embedder: Arc<dyn EmbeddingProvider>,
    workers: Vec<AgentWorker>,
    manager: ManagerAgent,
}

impl DyTopoCoordinator {
    pub async fn new(
        config: DyTopoConfig,
        llm_config: Config,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self> {
        println!("  Initializing workers...");
        let mut workers = Vec::new();
        for agent_id in &config.agent_ids {
            let prompt = config.agent_prompts.get(agent_id)
                .cloned()
                .unwrap_or_else(|| format!("You are agent '{}'.", agent_id));
            print!("    Creating worker '{}'... ", agent_id);
            std::io::Write::flush(&mut std::io::stdout()).ok();
            let worker = AgentWorker::new(agent_id.clone(), &prompt, llm_config.clone()).await?;
            println!("ok");
            workers.push(worker);
        }
        print!("  Creating manager... ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        let manager = ManagerAgent::new(llm_config.clone()).await?;
        println!("ok");

        Ok(Self { config, llm_config, embedder, workers, manager })
    }

    pub async fn run(&mut self, task: &str) -> Result<String> {
        let total_start = Instant::now();
        let mut current_goal = task.to_string();
        let mut all_public_messages: Vec<String> = Vec::new();

        println!("\n--- DyTopo Protocol Start ---");
        println!("  Task: {}", task);
        println!("  Workers: {:?}", self.config.agent_ids);
        println!("  Max rounds: {}, tau: {}, k_in: {}\n",
            self.config.max_rounds, self.config.tau_edge, self.config.k_in);

        for round in 1..=self.config.max_rounds {
            let round_start = Instant::now();
            println!("=== Round {}/{} ===", round, self.config.max_rounds);
            println!("  Goal: {}", current_goal);

            let round_outputs = self.run_workers(&current_goal, round).await?;

            // Collect descriptor pairs with debug info
            let descriptor_pairs: Vec<DescriptorPair> = round_outputs.iter().map(|(id, output)| {
                if output.query.is_empty() && output.key.is_empty() {
                    println!("    {} descriptors: (empty - no descriptor block parsed)", id);
                } else {
                    let q = if output.query.len() > 60 { &output.query[..60] } else { &output.query };
                    let k = if output.key.len() > 60 { &output.key[..60] } else { &output.key };
                    println!("    {} query: \"{}\"", id, q);
                    println!("    {} key:   \"{}\"", id, k);
                }
                DescriptorPair {
                    agent_id: id.clone(),
                    query: output.query.clone(),
                    key: output.key.clone(),
                }
            }).collect();

            print!("  Embedding descriptors... ");
            std::io::Write::flush(&mut std::io::stdout()).ok();
            let embedded = self.embed_descriptors(&descriptor_pairs).await?;
            println!("done ({} pairs)", embedded.len());

            let builder = TopologyBuilder::new(self.config.tau_edge, self.config.k_in);
            let topology = builder.build(&embedded);
            println!("  Topology: {} edges across {} agents",
                topology.edge_count(), topology.agent_count());

            for agent_id in &self.config.agent_ids {
                let senders = topology.senders_to(agent_id);
                if !senders.is_empty() {
                    let sender_strs: Vec<String> = senders.iter()
                        .map(|e| format!("{}({:.2})", e.from, e.score))
                        .collect();
                    println!("    {} <- [{}]", agent_id, sender_strs.join(", "));
                }
            }

            let private_messages: HashMap<String, String> = round_outputs.iter()
                .map(|(id, output)| (id.clone(), output.private_msg.clone()))
                .collect();
            let inboxes = message::route_messages(&topology, &private_messages, round);

            for (agent_id, inbox) in &inboxes {
                if !inbox.is_empty() {
                    println!("    {} received {} messages", agent_id, inbox.len());
                }
            }

            let public_messages: Vec<(String, String)> = round_outputs.iter()
                .map(|(id, output)| (id.clone(), output.public_msg.clone()))
                .collect();

            for (id, msg) in &public_messages {
                if msg.is_empty() {
                    println!("  Public [{}]: (empty)", id);
                } else {
                    let preview = if msg.len() > 120 { &msg[..120] } else { msg.as_str() };
                    println!("  Public [{}]: {}", id, preview);
                }
                all_public_messages.push(format!("[Round {} - {}] {}", round, id, msg));
            }

            print!("  Manager evaluating... ");
            std::io::Write::flush(&mut std::io::stdout()).ok();
            let decision = self.manager.evaluate_round(round, task, &public_messages).await?;
            println!("{}", if decision.halt { "HALT" } else { "CONTINUE" });
            println!("    Reason: {}", decision.reason);

            let round_elapsed = round_start.elapsed();
            println!("  Round {} completed in {:.1}s\n", round, round_elapsed.as_secs_f64());

            if decision.halt {
                println!("--- DyTopo Protocol Complete ---");
                println!("  Rounds: {}", round);
                println!("  Total time: {:.1}s", total_start.elapsed().as_secs_f64());
                return Ok(all_public_messages.join("\n\n"));
            }

            if !decision.next_goal.is_empty() {
                current_goal = decision.next_goal;
            }
        }

        println!("--- DyTopo Protocol Complete (max rounds reached) ---");
        println!("  Total time: {:.1}s", total_start.elapsed().as_secs_f64());
        Ok(all_public_messages.join("\n\n"))
    }

    async fn run_workers(
        &mut self,
        goal: &str,
        round: usize,
    ) -> Result<Vec<(String, super::descriptor::AgentRoundOutput)>> {
        let mut outputs = Vec::new();
        for worker in &mut self.workers {
            let agent_id = worker.agent_id.clone();
            print!("  Running worker '{}'... ", agent_id);
            std::io::Write::flush(&mut std::io::stdout()).ok();
            let start = Instant::now();
            let output = worker.run_round(goal, &[], round).await?;
            let elapsed = start.elapsed();
            println!("done ({:.1}s)", elapsed.as_secs_f64());
            outputs.push((agent_id, output));
        }
        Ok(outputs)
    }

    async fn embed_descriptors(
        &self,
        pairs: &[DescriptorPair],
    ) -> Result<Vec<EmbeddedDescriptors>> {
        let mut texts: Vec<String> = Vec::new();
        for pair in pairs {
            texts.push(pair.query.clone());
            texts.push(pair.key.clone());
        }
        let vectors = self.embedder.embed_batch(&texts).await?;
        let mut embedded = Vec::new();
        for (i, pair) in pairs.iter().enumerate() {
            embedded.push(EmbeddedDescriptors {
                agent_id: pair.agent_id.clone(),
                query_vec: vectors[i * 2].clone(),
                key_vec: vectors[i * 2 + 1].clone(),
            });
        }
        Ok(embedded)
    }
}
