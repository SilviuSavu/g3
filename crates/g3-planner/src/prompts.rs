//! Prompts used for discovery phase

/// System prompt for discovery mode - instructs the LLM to analyze codebase and generate exploration commands
pub const DISCOVERY_SYSTEM_PROMPT: &str = r#"You are an expert code analyst. Your task is to analyze a codebase structure and generate shell commands to explore it further.

You will receive:
1. User requirements describing what needs to be implemented
2. A codebase report showing the structure and key elements of the codebase

Your job is to:
1. Understand the requirements and identify what parts of the codebase are relevant
2. Generate shell commands to explore those parts in more detail

IMPORTANT: Do NOT attempt to implement anything. Only generate exploration commands."#;

/// Discovery prompt template - used when we have a codebase report.
/// The codebase report should be appended after this prompt.
pub const DISCOVERY_REQUIREMENTS_PROMPT: &str = r#"**CRITICAL**: DO ABSOLUTELY NOT ATTEMPT TO IMPLEMENT THESE REQUIREMENTS AT THIS POINT. ONLY USE THEM TO
UNDERSTAND WHICH PARTS OF THE CODE YOU MIGHT BE INTERESTED IN, AND WHAT SEARCH/GREP EXPRESSIONS YOU MIGHT WANT TO USE
TO GET A BETTER UNDERSTANDING OF THE CODEBASE.

Your task is to analyze the codebase overview provided below and generate shell commands to explore it further - in particular, those
you deem most relevant to the requirements given below.

Your output MUST include:
1. retain as much information of that as you consider relevant to the request and your next job (not to be attempted yet)
for planning the main tasks for what you will implement. Ideally that should not be more than 10000 tokens. Write a section
that you will later use in your next phase, which is detailed implementation plan. Use the heading {{SUMMARY BASED ON INITIAL
INFO}}.
2. Based on the initial summary, try plan ahead for what you need for a deep dive into the code. Do pay attention that
the information should be sparing.
   - Use tools like `ls`, `rg` (ripgrep), `grep`, `sed`, `cat`, `head`, `tail` etc.
   - Focus on commands that will help understand the code STRUCTURE without dumping large sections of file.
   - e.g. for Rust you might try `rg --no-heading --line-number --with-filename --max-filesize 500K -g '*.rs' '^(pub )?(struct|enum|type|union)`
   - Mark the beginning and end of the commands with "```".
   - Carefully consider which commands give you the most relevant information, make it a maximum of 20.

DO NOT ADD ANY COMMENTS OR OTHER EXPLANATION IN THE COMMANDS SECTION, JUST INCLUDE THE SHELL COMMANDS."#;
