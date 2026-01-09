use crate::ContextWindow;

/// Result of a task execution containing both the response and the context window
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// The actual response content from the task execution
    pub response: String,
    /// The complete context window at the time of completion
    pub context_window: ContextWindow,
}

impl TaskResult {
    pub fn new(response: String, context_window: ContextWindow) -> Self {
        Self {
            response,
            context_window,
        }
    }

    /// Extract a summary from the response (for coach feedback in autonomous mode)
    /// This looks for the last substantial text block in the response.
    /// Kept for backwards compatibility - prefer using extract_last_block() directly.
    pub fn extract_summary(&self) -> String {
        self.extract_last_block()
    }

    /// Legacy method - extract the final_output content from the response
    /// Now just delegates to extract_last_block() for backwards compatibility
    pub fn extract_final_output(&self) -> String {
        // Remove any timing information at the end
        let content_without_timing = if let Some(timing_pos) = self.response.rfind("\n⏱️") {
            &self.response[..timing_pos]
        } else {
            &self.response
        };

        // For backwards compatibility, still check for final_output marker
        // but primarily just return the last substantial block
        self.extract_last_block_from(content_without_timing)
    }

    /// Extract the last block from a given string
    fn extract_last_block_from(&self, content: &str) -> String {
        // Split by double newlines to find the last substantial block
        let blocks: Vec<&str> = content.split("\n\n").collect();

        // Find the last non-empty block that isn't just whitespace
        blocks
            .iter()
            .rev()
            .find(|block| !block.trim().is_empty())
            .map(|block| block.trim().to_string())
            .unwrap_or_else(|| content.trim().to_string())
    }

    /// Extract the last block from the response (for coach feedback in autonomous mode)
    /// This looks for the final_output content which is the last substantial block
    pub fn extract_last_block(&self) -> String {
        // Remove any timing information at the end
        let content_without_timing = if let Some(timing_pos) = self.response.rfind("\n⏱️") {
            &self.response[..timing_pos]
        } else {
            &self.response
        };

        // Split by double newlines to find the last substantial block
        let blocks: Vec<&str> = content_without_timing.split("\n\n").collect();

        // Find the last non-empty block that isn't just whitespace
        blocks
            .iter()
            .rev()
            .find(|block| !block.trim().is_empty())
            .map(|block| block.trim().to_string())
            .unwrap_or_else(|| {
                // Fallback: if we can't find a clear block, take the whole thing
                content_without_timing.trim().to_string()
            })
    }

    /// Check if the response contains an approval (for autonomous mode)
    pub fn is_approved(&self) -> bool {
        self.extract_final_output()
            .contains("IMPLEMENTATION_APPROVED")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_last_block() {
        // Test case 1: Response with timing info
        let context_window = ContextWindow::new(1000);
        let response_with_timing =
            "Some initial content\n\nFinal block content\n\n⏱️ 2.3s | 💭 1.2s".to_string();
        let result = TaskResult::new(response_with_timing, context_window.clone());
        assert_eq!(result.extract_last_block(), "Final block content");

        // Test case 2: Response without timing
        let response_no_timing = "Some initial content\n\nFinal block content".to_string();
        let result = TaskResult::new(response_no_timing, context_window.clone());
        assert_eq!(result.extract_last_block(), "Final block content");

        // Test case 3: Response with IMPLEMENTATION_APPROVED
        let response_approved = "Some content\n\nIMPLEMENTATION_APPROVED".to_string();
        let result = TaskResult::new(response_approved, context_window.clone());
        assert!(result.is_approved());

        // Test case 4: Response without approval
        let response_not_approved = "Some content\n\nNeeds more work".to_string();
        let result = TaskResult::new(response_not_approved, context_window);
        assert!(!result.is_approved());
    }

    #[test]
    fn test_extract_last_block_edge_cases() {
        let context_window = ContextWindow::new(1000);

        // Test empty response
        let empty_response = "".to_string();
        let result = TaskResult::new(empty_response, context_window.clone());
        assert_eq!(result.extract_last_block(), "");

        // Test single block
        let single_block = "Just one block".to_string();
        let result = TaskResult::new(single_block, context_window.clone());
        assert_eq!(result.extract_last_block(), "Just one block");

        // Test multiple empty blocks
        let multiple_empty = "\n\n\n\nSome content\n\n\n\n".to_string();
        let result = TaskResult::new(multiple_empty, context_window);
        assert_eq!(result.extract_last_block(), "Some content");
    }

    #[test]
    fn test_extract_final_output() {
        let context_window = ContextWindow::new(1000);

        // Test case 1: Response with multiple blocks - extracts last substantial block
        let response_with_blocks = "Analyzing files...\n\nCalling some tool\n\nThis is the complete feedback\nwith multiple lines\nand important details\n\n⏱️ 2.3s".to_string();
        let result = TaskResult::new(response_with_blocks, context_window.clone());
        assert_eq!(
            result.extract_final_output(),
            "This is the complete feedback\nwith multiple lines\nand important details"
        );

        // Test case 2: Response with IMPLEMENTATION_APPROVED as last block
        let response_approved =
            "Review complete\n\nAnalysis done\n\nIMPLEMENTATION_APPROVED".to_string();
        let result = TaskResult::new(response_approved, context_window.clone());
        assert_eq!(result.extract_final_output(), "IMPLEMENTATION_APPROVED");
        assert!(result.is_approved());

        // Test case 3: Response with detailed feedback as last block
        let response_feedback = "Checking implementation...\n\nAnalysis complete\n\nThe following issues need to be addressed:\n1. Missing error handling in main.rs\n2. Tests are not comprehensive\n3. Documentation needs improvement\n\nPlease fix these issues.".to_string();
        let result = TaskResult::new(response_feedback, context_window.clone());
        let extracted = result.extract_final_output();
        // Now extracts just the last block (after the last \n\n)
        assert!(extracted.contains("Please fix these issues."));
        assert!(!result.is_approved());

        // Test case 4: Simple response - extracts last block
        let response_simple = "Some analysis\n\nFinal thoughts here".to_string();
        let result = TaskResult::new(response_simple, context_window.clone());
        assert_eq!(result.extract_final_output(), "Final thoughts here");

        // Test case 5: Empty response
        let empty_response = "".to_string();
        let result = TaskResult::new(empty_response, context_window);
        assert_eq!(result.extract_final_output(), "");
    }
}
