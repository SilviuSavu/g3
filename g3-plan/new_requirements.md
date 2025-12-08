1.

In planner.rs Show coach feedback: up to 25 lines

coach_feedback = result.response;
print_msg(&format!("📝 Coach feedback: {} chars", coach_feedback.len()));

2.

I can’t find where the TODO file is written during implementation in planning mode. Please check that it’s written to the g3-plan directory.
It looks like there are explicit instructions to delete the TODO file when complete, potentially in player mode. DO NOT ALLOW it to be deleted when in planner mode since we want to copy it for history.

3.
Make sure to write the “GIT COMMIT (<message>)”  to the planner_history.txt file *immediately before* doing the actual commit (not after, like the current implementation  does).

4.  In planner mode, do not write a new line in UI writer for each tool call. Instead keep a single line that says “thinking....” While the llm is working.  Keep each update on a single line (use backspace or something to erase the last update?) and show the context window size and that we’re waiting for the llm to finish tool calls. HOWEVER, DO PRINT to the UI all non-tool comments (text messages) that the llm sends (that’s currently not happening).

5. Logs are written to the <codepath> directory. Instead write them to the workspace path.

