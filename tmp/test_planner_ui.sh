#!/bin/bash
set -e

# Clean logs first
rm -rf ~/RustroverProjects/g3/logs/*.log ~/RustroverProjects/g3/logs/*.txt 2>/dev/null || true

# Create test requirements file
mkdir -p /tmp/g3-test-planning/g3-plan
cat > /tmp/g3-test-planning/g3-plan/new_requirements.md <<'EOF'
Simple test task: List all .rs files in the src directory.
EOF

# Initialize git repo for test (planning mode requires git)
cd /tmp/g3-test-planning
if [ ! -d .git ]; then
    git init
    git config user.name "Test User"
    git config user.email "test@example.com"
    git add .
    git commit -m "Initial commit" || true
fi

echo "Test environment ready at /tmp/g3-test-planning"
echo "Run: cd /tmp && ~/RustroverProjects/g3/target/release/g3 --planning --codepath /tmp/g3-test-planning --no-git"
