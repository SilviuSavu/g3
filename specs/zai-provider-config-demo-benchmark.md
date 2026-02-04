# Plan: Z.ai Provider Configuration, Demo, and Benchmark

## Task Description

Configure the newly added Z.ai (Zhipu AI) provider with GLM-4.7 support, create a demonstration example, and establish benchmarks to verify functionality and measure performance. The provider implementation already exists (`crates/g3-providers/src/zai.rs`) with full support for streaming, tool calling, and thinking mode.

## Objective

- Add Z.ai configuration examples to the config.example.toml
- Create a runnable example demonstrating the Z.ai provider
- Build a simple benchmark comparing Z.ai against other providers (response latency, token throughput)
- Document the provider in the codebase

## Relevant Files

Use these files to complete the task:

- `config.example.toml` - Add Z.ai configuration section with all supported options
- `crates/g3-providers/src/zai.rs` - Reference for provider capabilities (already complete)
- `crates/g3-config/src/lib.rs` - ZaiConfig struct already defined (lines 111-124)
- `crates/g3-core/src/provider_registration.rs` - Registration logic already implemented (lines 188-210)
- `examples/verify_message_id.rs` - Reference pattern for creating examples

### New Files

- `examples/zai_demo.rs` - Demonstration example showing Z.ai provider usage
- `examples/provider_benchmark.rs` - Simple benchmark comparing providers

## Step by Step Tasks

IMPORTANT: Execute every step in order, top to bottom.

### 1. Update config.example.toml with Z.ai Configuration

- Add a new Z.ai provider section after the OpenAI-compatible section
- Include all configuration options from ZaiConfig:
  - `api_key` (required)
  - `model` (required, default: "glm-4.7")
  - `base_url` (optional, international vs China endpoint)
  - `max_tokens` (optional)
  - `temperature` (optional)
  - `enable_thinking` (optional, GLM-4.7 feature)
  - `preserve_thinking` (optional)
- Document both regional endpoints:
  - International: `https://api.z.ai/api/paas/v4`
  - China: `https://open.bigmodel.cn/api/paas/v4`

### 2. Create Z.ai Demo Example

- Create `examples/zai_demo.rs`
- Demonstrate:
  - Basic provider creation with default settings
  - Non-streaming completion
  - Streaming completion
  - Tool calling capability
  - Thinking mode (enable_thinking feature)
- Include clear output formatting showing each capability
- Handle errors gracefully with informative messages
- Support reading API key from environment variable `ZAI_API_KEY`

### 3. Create Provider Benchmark Example

- Create `examples/provider_benchmark.rs`
- Implement simple timing measurements for:
  - Time to first token (TTFT) for streaming
  - Total completion time
  - Tokens per second throughput
- Support benchmarking any configured provider via command-line argument
- Run multiple iterations and compute averages
- Output results in a clean tabular format
- Include a standard benchmark prompt (e.g., "Explain quicksort in 3 sentences")

### 4. Update Cargo.toml for Examples

- Add example entries in the root `Cargo.toml` if needed
- Ensure all dependencies are available (tokio, serde_json, etc.)

### 5. Add Documentation to Provider Module

- Add doc comments to `zai.rs` module-level docs referencing the example
- Add a note about how to run the demo: `cargo run --example zai_demo`

### 6. Validate the Implementation

- Run `cargo check` to ensure everything compiles
- Run `cargo test -p g3-providers` to verify Z.ai tests pass
- Verify the example compiles: `cargo build --example zai_demo`
- Verify the benchmark compiles: `cargo build --example provider_benchmark`

## Testing Strategy

1. **Unit Tests** (already exist in `zai.rs`):
   - Provider creation
   - Message conversion
   - Tool conversion
   - Request body generation with/without thinking
   - Streaming tool call accumulation

2. **Integration Test** (manual via demo):
   - Run `ZAI_API_KEY=your-key cargo run --example zai_demo`
   - Verify streaming output
   - Verify tool calling works
   - Verify thinking mode produces output

3. **Benchmark Validation**:
   - Run benchmark against at least one configured provider
   - Verify timing measurements are reasonable

## Acceptance Criteria

- [ ] `config.example.toml` includes Z.ai provider section with all options documented
- [ ] `examples/zai_demo.rs` compiles and demonstrates all provider features
- [ ] `examples/provider_benchmark.rs` compiles and produces timing results
- [ ] All existing tests continue to pass
- [ ] Demo can be run with `cargo run --example zai_demo` (with API key)
- [ ] Benchmark can be run with `cargo run --example provider_benchmark`

## Validation Commands

Execute these commands to validate the task is complete:

- `cargo check` - Ensure the codebase compiles
- `cargo test -p g3-providers` - Run provider tests including Z.ai tests
- `cargo build --example zai_demo` - Verify demo example compiles
- `cargo build --example provider_benchmark` - Verify benchmark compiles
- `grep -q "zai" config.example.toml` - Verify Z.ai config section exists

## Notes

### API Key Requirements

- Obtain Z.ai API key from https://open.bigmodel.cn/ (China) or https://z.ai/ (International)
- Set via environment variable: `export ZAI_API_KEY=your-key`
- Or configure in `~/.config/g3/config.toml`:
  ```toml
  [providers.zai.default]
  api_key = "your-api-key"
  model = "glm-4.7"
  enable_thinking = true
  ```

### GLM-4.7 Features

- Supports extended thinking mode with `reasoning_content` in responses
- 128K context window
- Native tool/function calling
- OpenAI-compatible API format with Z.ai extensions

### Regional Endpoints

- International API (`api.z.ai`) - Better latency for non-China users
- China API (`open.bigmodel.cn`) - Original endpoint for China-based users
