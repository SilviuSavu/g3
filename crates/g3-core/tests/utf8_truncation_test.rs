//! UTF-8 Safe Truncation Tests
//!
//! CHARACTERIZATION: These tests verify that string truncation operations
//! handle multi-byte UTF-8 characters correctly without panicking.
//!
//! What these tests protect:
//! - Truncation of strings containing emoji, CJK characters, and other multi-byte chars
//! - Word-boundary truncation with multi-byte characters
//! - Edge cases at exact character boundaries
//!
//! What these tests intentionally do NOT assert:
//! - Internal implementation details of truncation
//! - Exact output format (only that it doesn't panic and is valid UTF-8)
//!
//! Related commits:
//! - f30f145: Fix UTF-8 panics and inconsistent retry logic

use g3_core::acd::Fragment;
use g3_providers::{Message, MessageRole};

// =============================================================================
// Test: Fragment topic extraction with multi-byte characters
// =============================================================================

mod topic_extraction_utf8 {
    use super::*;

    /// Helper to create a fragment and extract its topics via stub generation
    fn extract_topics_from_messages(messages: Vec<Message>) -> Vec<String> {
        let fragment = Fragment::new(messages, None);
        // Topics are embedded in the stub, so we verify the fragment was created
        // without panicking and has valid data
        fragment.topics.clone()
    }

    /// Test that emoji in user messages don't cause panics
    #[test]
    fn test_emoji_in_topic() {
        let messages = vec![
            Message::new(MessageRole::User, "🚀 Deploy the application to production".to_string()),
            Message::new(MessageRole::Assistant, "I'll help you deploy.".to_string()),
        ];
        
        let topics = extract_topics_from_messages(messages);
        
        // Should not panic and should contain the topic
        assert!(!topics.is_empty(), "Should extract at least one topic");
        assert!(topics[0].contains("🚀") || topics[0].contains("Deploy"), 
            "Topic should contain emoji or text: {:?}", topics);
    }

    /// Test that CJK characters don't cause panics
    #[test]
    fn test_cjk_characters_in_topic() {
        let messages = vec![
            Message::new(MessageRole::User, "请帮我实现一个用户认证模块".to_string()),
            Message::new(MessageRole::Assistant, "好的，我来帮你实现。".to_string()),
        ];
        
        let topics = extract_topics_from_messages(messages);
        
        // Should not panic
        assert!(!topics.is_empty(), "Should extract topic from CJK text");
    }

    /// Test that mixed ASCII and multi-byte characters work
    #[test]
    fn test_mixed_ascii_and_multibyte() {
        let messages = vec![
            Message::new(MessageRole::User, "Fix the bug in auth.rs • important ⚡ urgent".to_string()),
            Message::new(MessageRole::Assistant, "I'll fix it.".to_string()),
        ];
        
        let topics = extract_topics_from_messages(messages);
        
        // Should not panic
        assert!(!topics.is_empty(), "Should extract topic from mixed text");
    }

    /// Test long message with emoji that would be truncated
    #[test]
    fn test_long_message_with_emoji_truncation() {
        // Create a message longer than 50 characters with emoji scattered throughout
        let long_msg = "🔧 Fix the authentication bug in the login module that causes users to be logged out unexpectedly 🐛";
        assert!(long_msg.chars().count() > 50, "Test message should be > 50 chars");
        
        let messages = vec![
            Message::new(MessageRole::User, long_msg.to_string()),
            Message::new(MessageRole::Assistant, "I'll investigate.".to_string()),
        ];
        
        let topics = extract_topics_from_messages(messages);
        
        // Should not panic and topic should be truncated
        assert!(!topics.is_empty(), "Should extract truncated topic");
        // The topic should be valid UTF-8 (this would fail if truncated mid-character)
        let topic = &topics[0];
        assert!(topic.is_ascii() || topic.chars().count() > 0, "Topic should be valid UTF-8");
    }

    /// Test message with emoji at exactly the truncation boundary
    #[test]
    fn test_emoji_at_truncation_boundary() {
        // Create a message where an emoji would be at position 49-50
        // "a]" repeated to fill 48 chars, then emoji
        let prefix = "a".repeat(48);
        let msg = format!("{}🚀🔥 more text here", prefix);
        
        let messages = vec![
            Message::new(MessageRole::User, msg),
            Message::new(MessageRole::Assistant, "OK".to_string()),
        ];
        
        let topics = extract_topics_from_messages(messages);
        
        // Should not panic - the key test is that this doesn't crash
        assert!(!topics.is_empty());
    }

    /// Test that bullet points (•) don't cause issues
    #[test]
    fn test_bullet_points() {
        let messages = vec![
            Message::new(MessageRole::User, "Tasks: • item one • item two • item three • item four • item five".to_string()),
            Message::new(MessageRole::Assistant, "I see the tasks.".to_string()),
        ];
        
        let topics = extract_topics_from_messages(messages);
        
        // Should not panic
        assert!(!topics.is_empty());
    }

    /// Test combining characters (diacritics)
    #[test]
    fn test_combining_characters() {
        // é can be represented as e + combining acute accent
        let messages = vec![
            Message::new(MessageRole::User, "Café résumé naïve coöperate fiancée".to_string()),
            Message::new(MessageRole::Assistant, "Understood.".to_string()),
        ];
        
        let topics = extract_topics_from_messages(messages);
        
        // Should not panic
        assert!(!topics.is_empty());
    }
}

// =============================================================================
// Test: Fragment stub generation with multi-byte characters
// =============================================================================

mod stub_generation_utf8 {
    use super::*;

    /// Test that stub generation works with emoji in topics
    #[test]
    fn test_stub_with_emoji_topics() {
        let messages = vec![
            Message::new(MessageRole::User, "🎯 Implement feature X".to_string()),
            Message::new(MessageRole::Assistant, "Starting implementation.".to_string()),
            Message::new(MessageRole::User, "🔧 Now fix the tests".to_string()),
            Message::new(MessageRole::Assistant, "Fixing tests.".to_string()),
        ];
        
        let fragment = Fragment::new(messages, None);
        let stub = fragment.generate_stub();
        
        // Stub should be valid UTF-8 and contain expected elements
        assert!(stub.contains("DEHYDRATED CONTEXT"), "Stub should have header");
        assert!(stub.contains(&fragment.fragment_id), "Stub should have fragment ID");
        assert!(stub.contains("rehydrate"), "Stub should mention rehydrate");
    }

    /// Test stub with very long multi-byte topic that gets truncated
    #[test]
    fn test_stub_with_truncated_multibyte_topic() {
        // Create a long message with multi-byte chars that will be truncated
        let long_msg = "🔧 ".to_string() + &"修复".repeat(30); // Chinese chars, each 3 bytes
        
        let messages = vec![
            Message::new(MessageRole::User, long_msg),
            Message::new(MessageRole::Assistant, "好的".to_string()),
        ];
        
        let fragment = Fragment::new(messages, None);
        let stub = fragment.generate_stub();
        
        // Should not panic and stub should be valid
        assert!(stub.contains("DEHYDRATED CONTEXT"));
    }
}

// =============================================================================
// Test: Edge cases for character counting vs byte counting
// =============================================================================

mod char_vs_byte_edge_cases {
    use super::*;

    /// Test that we count characters, not bytes
    /// A string of 50 emoji is 50 characters but 200 bytes
    #[test]
    fn test_emoji_string_character_count() {
        let emoji_50 = "🔥".repeat(50);
        assert_eq!(emoji_50.chars().count(), 50, "Should be 50 characters");
        assert_eq!(emoji_50.len(), 200, "Should be 200 bytes (4 bytes per emoji)");
        
        let messages = vec![
            Message::new(MessageRole::User, emoji_50),
            Message::new(MessageRole::Assistant, "OK".to_string()),
        ];
        
        let fragment = Fragment::new(messages, None);
        
        // Should not panic - if we used byte slicing, this would crash
        let _stub = fragment.generate_stub();
    }

    /// Test exactly 50 characters (no truncation needed)
    #[test]
    fn test_exactly_50_chars() {
        let msg = "a".repeat(50);
        assert_eq!(msg.chars().count(), 50);
        
        let messages = vec![
            Message::new(MessageRole::User, msg),
            Message::new(MessageRole::Assistant, "OK".to_string()),
        ];
        
        let topics = Fragment::new(messages, None).topics;
        
        // Should not have "..." suffix since it's exactly 50
        assert!(!topics.is_empty());
        // Topic should be the full message or close to it
    }

    /// Test 51 characters (truncation needed)
    #[test]
    fn test_51_chars_triggers_truncation() {
        let msg = "a".repeat(51);
        assert_eq!(msg.chars().count(), 51);
        
        let messages = vec![
            Message::new(MessageRole::User, msg),
            Message::new(MessageRole::Assistant, "OK".to_string()),
        ];
        
        let topics = Fragment::new(messages, None).topics;
        
        // Should have truncation
        assert!(!topics.is_empty());
        let topic = &topics[0];
        assert!(topic.ends_with("..."), "Should be truncated: {}", topic);
    }

    /// Test string with 3-byte UTF-8 characters (CJK)
    #[test]
    fn test_3byte_utf8_chars() {
        // Each Chinese character is 3 bytes
        let cjk_60 = "中".repeat(60); // 60 chars, 180 bytes
        assert_eq!(cjk_60.chars().count(), 60);
        assert_eq!(cjk_60.len(), 180);
        
        let messages = vec![
            Message::new(MessageRole::User, cjk_60),
            Message::new(MessageRole::Assistant, "好".to_string()),
        ];
        
        let fragment = Fragment::new(messages, None);
        
        // Should not panic
        let _stub = fragment.generate_stub();
        assert!(!fragment.topics.is_empty());
    }

    /// Test string with 2-byte UTF-8 characters (Latin extended)
    #[test]
    fn test_2byte_utf8_chars() {
        // Each accented character is 2 bytes
        let accented_60 = "é".repeat(60); // 60 chars, 120 bytes
        assert_eq!(accented_60.chars().count(), 60);
        assert_eq!(accented_60.len(), 120);
        
        let messages = vec![
            Message::new(MessageRole::User, accented_60),
            Message::new(MessageRole::Assistant, "OK".to_string()),
        ];
        
        let fragment = Fragment::new(messages, None);
        
        // Should not panic
        let _stub = fragment.generate_stub();
        assert!(!fragment.topics.is_empty());
    }
}
