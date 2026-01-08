//! Quick test to verify syntax highlighting works
//! Run with: cargo test -p g3-cli --test test_final_output -- --nocapture

use std::io::{self, Write};

// We'll directly test the syntax_highlight module's public function
// by importing it and calling it with a MadSkin

#[test]
fn test_syntax_highlighting_visual() {
    // Import what we need
    use termimad::MadSkin;
    
    // Create the test markdown
    let test_markdown = r##"# Task Completed Successfully

Here's a summary of what was accomplished:

## Rust Code Example

Created a new function to handle user authentication:

```rust
use std::collections::HashMap;

/// Authenticates a user with the given credentials
pub async fn authenticate(username: &str, password: &str) -> Result<User, AuthError> {
    let hash = hash_password(password)?;
    
    if let Some(user) = db.find_user(username).await? {
        if user.password_hash == hash {
            Ok(user)
        } else {
            Err(AuthError::InvalidPassword)
        }
    } else {
        Err(AuthError::UserNotFound)
    }
}
```

## Python Example

Also added a Python script for data processing:

```python
import pandas as pd
from typing import List, Dict

def process_data(items: List[Dict]) -> pd.DataFrame:
    """Process raw items into a cleaned DataFrame."""
    df = pd.DataFrame(items)
    df['timestamp'] = pd.to_datetime(df['timestamp'])
    df = df.dropna(subset=['value'])
    return df.sort_values('timestamp')
```

## JavaScript/TypeScript

Frontend component:

```typescript
interface User {
  id: string;
  name: string;
  email: string;
}

const UserCard: React.FC<{ user: User }> = ({ user }) => {
  return (
    <div className="user-card">
      <h3>{user.name}</h3>
      <p>{user.email}</p>
    </div>
  );
};
```

## Shell Commands

Deployment script:

```bash
#!/bin/bash
set -euo pipefail

echo "Building project..."
cargo build --release

echo "Running tests..."
cargo test --all

echo "Deploying to production..."
rsync -avz ./target/release/app server:/opt/app/
```

## JSON Configuration

```json
{
  "name": "my-project",
  "version": "1.0.0",
  "dependencies": {
    "serde": "1.0",
    "tokio": { "version": "1.0", "features": ["full"] }
  }
}
```

## Other Markdown Features

This section tests that **bold text**, *italic text*, and `inline code` still work correctly.

### Lists

- First item
- Second item with **bold**
- Third item with `code`

### Numbered List

1. Step one
2. Step two
3. Step three

### Blockquote

> This is a blockquote that should be rendered
> with proper styling by termimad.

### Table

| Language | Extension | Use Case |
|----------|-----------|----------|
| Rust | .rs | Systems |
| Python | .py | Scripts |
| TypeScript | .ts | Frontend |

## Code Without Language

```
This is a code block without a language specified.
It should still be rendered as code, just without
syntax highlighting.
```

## Final Notes

All changes have been tested and verified. The implementation:

- ✅ Handles multiple languages
- ✅ Preserves markdown formatting
- ✅ Works with nested structures
- ✅ Gracefully handles edge cases
"##;

    // Create a styled markdown skin (same as in print_final_output)
    let mut skin = MadSkin::default();
    skin.bold.set_fg(termimad::crossterm::style::Color::Green);
    skin.italic.set_fg(termimad::crossterm::style::Color::Cyan);
    skin.headers[0].set_fg(termimad::crossterm::style::Color::Magenta);
    skin.headers[1].set_fg(termimad::crossterm::style::Color::Magenta);

    // Print header
    println!("\n\x1b[1;35m━━━ Summary ━━━\x1b[0m\n");

    // Use the syntax highlighting renderer
    let rendered = g3_cli::syntax_highlight::render_markdown_with_highlighting(test_markdown, &skin);
    print!("{}", rendered);

    // Print footer
    println!("\n\x1b[1;35m━━━━━━━━━━━━━━━\x1b[0m");
    
    let _ = io::stdout().flush();
}
