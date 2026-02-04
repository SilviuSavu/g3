//! Auto-detection of LSP servers based on project and file types.
//!
//! This module provides utilities to detect which language server to use
//! based on file extensions and project markers.

use crate::types::LspServerConfig;
use std::path::{Path, PathBuf};

/// Detect which language a file belongs to based on its extension.
///
/// # Returns
/// The language identifier (e.g., "rust", "typescript", "python") or None if unknown.
pub fn detect_language(file_path: &Path) -> Option<String> {
    let ext = file_path.extension()?.to_str()?;
    match ext {
        "rs" => Some("rust".to_string()),
        "ts" | "tsx" | "js" | "jsx" | "mts" | "cts" => Some("typescript".to_string()),
        "py" | "pyi" => Some("python".to_string()),
        "go" => Some("go".to_string()),
        "java" => Some("java".to_string()),
        "c" | "h" => Some("c".to_string()),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp".to_string()),
        "zig" => Some("zig".to_string()),
        "lua" => Some("lua".to_string()),
        "rb" => Some("ruby".to_string()),
        "swift" => Some("swift".to_string()),
        "kt" | "kts" => Some("kotlin".to_string()),
        "cs" => Some("csharp".to_string()),
        _ => None,
    }
}

/// Detect project root by finding marker files.
///
/// Walks up the directory tree from `start` looking for any of the `markers` files.
/// Returns the directory containing the first marker found, or None if none found.
///
/// # Arguments
/// * `start` - Starting directory or file path
/// * `markers` - List of marker file names to look for (e.g., "Cargo.toml", "package.json")
///
/// # Returns
/// The path to the project root directory, or None if no markers found.
pub fn find_project_root(start: &Path, markers: &[&str]) -> Option<PathBuf> {
    // If start is a file, get its parent directory
    let mut current = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };

    loop {
        for marker in markers {
            let marker_path = current.join(marker);
            if marker_path.exists() {
                return Some(current);
            }
        }

        // Move up to parent directory
        match current.parent() {
            Some(parent) if parent != current => {
                current = parent.to_path_buf();
            }
            _ => break,
        }
    }

    None
}

/// Get default server configuration for a language.
///
/// Returns a pre-configured `LspServerConfig` for known languages,
/// or None for unsupported languages.
pub fn default_server_config(language: &str) -> Option<LspServerConfig> {
    match language {
        "rust" => Some(LspServerConfig::rust_analyzer()),
        "typescript" => Some(LspServerConfig::typescript()),
        "python" => Some(LspServerConfig::python()),
        "go" => Some(LspServerConfig::go()),
        "c" | "cpp" => Some(LspServerConfig::clangd()),
        "zig" => Some(LspServerConfig::zls()),
        _ => None,
    }
}

/// Default root markers for each language.
///
/// Returns a list of file names that typically indicate the project root
/// for the given language.
pub fn root_markers(language: &str) -> &'static [&'static str] {
    match language {
        "rust" => &["Cargo.toml", "rust-project.json"],
        "typescript" => &["package.json", "tsconfig.json", "jsconfig.json"],
        "python" => &["pyproject.toml", "setup.py", "setup.cfg", "requirements.txt", "Pipfile"],
        "go" => &["go.mod", "go.work"],
        "java" => &["pom.xml", "build.gradle", "build.gradle.kts"],
        "c" | "cpp" => &["compile_commands.json", "CMakeLists.txt", "Makefile", ".clangd"],
        "kotlin" => &["build.gradle", "build.gradle.kts", "pom.xml"],
        "ruby" => &["Gemfile", ".ruby-version"],
        "swift" => &["Package.swift", ".xcodeproj", ".xcworkspace"],
        "csharp" => &[".csproj", ".sln"],
        "lua" => &[".luarc.json", ".luacheckrc"],
        "zig" => &["build.zig", "build.zig.zon"],
        _ => &[],
    }
}

/// Find the project root for a given file, using language-specific markers.
///
/// Combines `detect_language`, `root_markers`, and `find_project_root` into
/// a single convenience function.
///
/// # Arguments
/// * `file_path` - Path to a file in the project
///
/// # Returns
/// A tuple of (language, project_root) if both can be determined, or None.
pub fn detect_project(file_path: &Path) -> Option<(String, PathBuf)> {
    let language = detect_language(file_path)?;
    let markers = root_markers(&language);
    let root = find_project_root(file_path, markers)?;
    Some((language, root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language(Path::new("main.rs")), Some("rust".to_string()));
        assert_eq!(detect_language(Path::new("app.ts")), Some("typescript".to_string()));
        assert_eq!(detect_language(Path::new("app.tsx")), Some("typescript".to_string()));
        assert_eq!(detect_language(Path::new("main.py")), Some("python".to_string()));
        assert_eq!(detect_language(Path::new("main.go")), Some("go".to_string()));
        assert_eq!(detect_language(Path::new("Main.java")), Some("java".to_string()));
        assert_eq!(detect_language(Path::new("main.c")), Some("c".to_string()));
        assert_eq!(detect_language(Path::new("main.cpp")), Some("cpp".to_string()));
        assert_eq!(detect_language(Path::new("unknown.xyz")), None);
        assert_eq!(detect_language(Path::new("no_extension")), None);
    }

    #[test]
    fn test_root_markers() {
        assert!(root_markers("rust").contains(&"Cargo.toml"));
        assert!(root_markers("typescript").contains(&"package.json"));
        assert!(root_markers("python").contains(&"pyproject.toml"));
        assert!(root_markers("go").contains(&"go.mod"));
        assert!(root_markers("unknown").is_empty());
    }

    #[test]
    fn test_default_server_config() {
        let rust_config = default_server_config("rust");
        assert!(rust_config.is_some());
        let config = rust_config.unwrap();
        assert_eq!(config.language_id, "rust");
        assert_eq!(config.command, "rust-analyzer");

        let ts_config = default_server_config("typescript");
        assert!(ts_config.is_some());

        assert!(default_server_config("unknown").is_none());
    }
}
