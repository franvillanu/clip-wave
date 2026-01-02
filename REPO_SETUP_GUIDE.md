# Repository Setup Guide - Specs-Driven Development (SDD)

This comprehensive guide ensures all new repositories follow the same SDD structure and workflow as established in ClipWave and Nautilus.

## Table of Contents

1. [Initial Repository Creation](#initial-repository-creation)
2. [Branch Configuration](#branch-configuration)
3. [Documentation Structure](#documentation-structure)
4. [Branch Protection Rules](#branch-protection-rules)
5. [Git Workflow](#git-workflow)
6. [Verification Checklist](#verification-checklist)

---

## Initial Repository Creation

### Option 1: Creating from Scratch

```bash
# Initialize git repository locally
git init
git add .
git commit -m "Initial commit - [Project Name]"

# Create GitHub repository using gh CLI
gh repo create [repo-name] --public --source=. --description="[Project description]" --push
```

### Option 2: Existing Local Repository

```bash
# If you already have a local repository without remote
gh repo create [repo-name] --public --source=. --description="[Project description]" --push
```

---

## Branch Configuration

### Rename master to main

If your repository uses `master` as the default branch, rename it to `main`:

```bash
# Rename local branch
git branch -m master main

# Push new branch to remote
git push -u origin main

# Set main as default branch on GitHub
gh repo edit --default-branch main

# Delete old master branch from remote
git push origin --delete master
```

**Verification:**
```bash
git branch --show-current  # Should show 'main'
```

---

## Documentation Structure

Create three essential documentation files following the SDD approach:

### 1. CLAUDE.md - Development Protocols

**Purpose:** Token-efficient development protocols for Claude Code

**Template Structure:**
```markdown
# CLAUDE.md - [Project Name] Development Protocol

## Core Principles
- 10x Token Reduction Strategy
- Never re-read cached files
- Use Grep before reading entire files
- Reference specs instead of full code

## Project Overview
[Brief description of project and tech stack]

## Git Workflow (MANDATORY)
- Main branch is protected
- Always work on feature branches
- Users create pull requests for all changes

## Code Modification Guidelines
[Guidelines specific to your project]

## Token Optimization Hierarchy
[Optimization strategies]

## Supporting Documentation
[Links to other docs]

## Best Practices
[DO and DON'T lists]
```

### 2. CODEX.md - Quick Reference Guide

**Purpose:** Comprehensive reference for development patterns and structure

**Template Structure:**
```markdown
# CODEX.md - [Project Name] Quick Reference Guide

## Project Overview
[Description and tech stack]

## Project Structure
[Directory tree and file descriptions]

## Development Commands
[npm/cargo/etc. commands]

## Git Workflow (MANDATORY)
[Branch naming and workflow steps]

## Code Organization
[Component/module patterns]

## Best Practices
[DO and DON'T lists]

## Common Patterns
[Code snippets and examples]

## Testing Checklist
[Testing requirements]

## Deployment
[Build and deployment instructions]

## Quick Reference
[Table of common tasks]
```

### 3. README.md - Project Documentation

**Purpose:** Complete project overview and onboarding

**Template Structure:**
```markdown
# [Project Name] - [One-line description]

[Brief description paragraph]

## Features
- Feature 1
- Feature 2
- etc.

## Tech Stack
- Technology 1
- Technology 2
- etc.

## Quick Start

### Prerequisites
[Required tools and versions]

### Installation
[Step-by-step setup]

### Development Commands
[Common commands]

## Project Structure
[Directory overview]

## Development Philosophy: Specs-Driven Development (SDD)

[Project Name] follows **Specs-Driven Development**, emphasizing comprehensive documentation over scattered code comments.

### Key Documentation
| File | Purpose |
|------|---------|
| README.md | Project overview (this file) |
| CLAUDE.md | Development protocols for Claude Code |
| CODEX.md | Quick reference guide |

## Git Workflow

[Project Name] uses a **branch-based workflow** with protected main branch:

1. Main branch is protected
2. Feature branches for all changes
3. Pull requests for all merges
4. Descriptive commits

### Example Workflow
[Code example]

## Building for Production
[Build instructions]

## Contributing
[Contribution guidelines]

## Development Best Practices
[DO and DON'T lists]

## Resources
[Relevant documentation links]

## License
[License info]

## Contact
Author: Francisco Villanueva ([@franvillanu](https://github.com/franvillanu))
Repository: [github.com/franvillanu/[repo-name]](https://github.com/franvillanu/[repo-name])
```

---

## Branch Protection Rules

Enable branch protection for the main branch to prevent direct commits:

```bash
gh api repos/franvillanu/[repo-name]/branches/main/protection -X PUT \
  -H "Accept: application/vnd.github+json" \
  --input - <<'EOF'
{
  "required_status_checks": null,
  "enforce_admins": false,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": false,
    "require_code_owner_reviews": false,
    "required_approving_review_count": 0
  },
  "restrictions": null,
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "block_creations": false,
  "required_conversation_resolution": false,
  "lock_branch": false,
  "allow_fork_syncing": false
}
EOF
```

**What this does:**
- Requires pull requests for merging to main
- Prevents force pushes to main
- Prevents deletion of main branch
- Allows admin to bypass (so you can still merge your own PRs)

**Verification:**
```bash
gh api repos/franvillanu/[repo-name]/branches/main/protection | jq .
```

---

## Git Workflow

### Feature Branch Workflow

**Branch Naming Conventions:**
- `feature/description` - New features
- `fix/bug-name` - Bug fixes
- `refactor/what` - Code refactoring
- `docs/what` - Documentation updates
- `chore/what` - Maintenance tasks

**Standard Workflow:**

```bash
# 1. Check you're on main and it's up to date
git checkout main
git pull

# 2. Create feature branch
git checkout -b feature/my-feature

# 3. Make changes and commit
git add .
git commit -m "Descriptive message

- Detailed change 1
- Detailed change 2

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"

# 4. Push branch
git push -u origin feature/my-feature

# 5. Create PR via GitHub CLI
gh pr create --title "Feature: My feature" --body "Description of changes" --base main
```

### Commit Message Format

```
Brief summary (50 chars or less)

Detailed description of changes:
- What was changed
- Why it was changed
- Any notable implementation details

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
```

---

## .gitignore Additions

Add these to your `.gitignore` file:

```gitignore
# Claude settings
.claude/settings.local.json
```

Then remove from tracking if already committed:

```bash
git rm --cached .claude/settings.local.json
git commit -m "Remove settings.local.json from tracking"
```

---

## Verification Checklist

After setting up a new repository, verify:

- [ ] Repository created on GitHub
- [ ] Default branch is `main` (not master)
- [ ] Branch protection enabled on main
- [ ] CLAUDE.md created and committed
- [ ] CODEX.md created and committed
- [ ] README.md updated with SDD structure
- [ ] .gitignore includes `.claude/settings.local.json`
- [ ] settings.local.json removed from tracking (if applicable)
- [ ] Feature branch workflow tested
- [ ] Pull request created successfully
- [ ] Cannot push directly to main (protection verified)

**Quick Verification Commands:**

```bash
# Check default branch
gh repo view --json defaultBranchRef --jq .defaultBranchRef.name

# Check branch protection
gh api repos/franvillanu/[repo-name]/branches/main/protection

# List files
ls -la | grep -E "(CLAUDE|CODEX|README)"

# Test branch protection (should fail)
git checkout main
echo "test" >> test.txt
git add test.txt
git commit -m "Test commit"
git push  # This should be rejected
```

---

## Quick Setup Script

Here's a complete script to set up a new repository:

```bash
#!/bin/bash

# Variables
REPO_NAME="your-repo-name"
REPO_DESC="Your project description"
PROJECT_NAME="Your Project Name"

# 1. Create repository
gh repo create $REPO_NAME --public --source=. --description="$REPO_DESC" --push

# 2. Rename branch if needed
git branch -m master main 2>/dev/null || true
git push -u origin main
gh repo edit --default-branch main
git push origin --delete master 2>/dev/null || true

# 3. Enable branch protection
gh api repos/franvillanu/$REPO_NAME/branches/main/protection -X PUT \
  -H "Accept: application/vnd.github+json" \
  --input - <<'EOF'
{
  "required_status_checks": null,
  "enforce_admins": false,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": false,
    "require_code_owner_reviews": false,
    "required_approving_review_count": 0
  },
  "restrictions": null,
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "block_creations": false,
  "required_conversation_resolution": false,
  "lock_branch": false,
  "allow_fork_syncing": false
}
EOF

# 4. Create feature branch for documentation
git checkout -b feature/sdd-documentation

# 5. Create documentation files (you'll need to customize these)
echo "Documentation files need to be created manually based on templates"
echo "Create: CLAUDE.md, CODEX.md, README.md"

# 6. Commit and push
git add CLAUDE.md CODEX.md README.md
git commit -m "Add SDD documentation structure"
git push -u origin feature/sdd-documentation

# 7. Create PR
gh pr create --title "Add SDD documentation" --body "Initial SDD setup" --base main

echo "✓ Repository setup complete!"
echo "✓ Branch protection enabled"
echo "✓ Documentation PR created"
echo "Next: Review and merge the PR"
```

---

## Common Issues and Solutions

### Issue: Can't push to main

**Symptom:** `remote: error: GH006: Protected branch update failed`

**Solution:** This is expected! Branch protection is working. Create a feature branch instead:
```bash
git checkout -b feature/my-changes
git push -u origin feature/my-changes
```

### Issue: Wrong default branch

**Symptom:** GitHub shows `master` instead of `main`

**Solution:**
```bash
gh repo edit --default-branch main
```

### Issue: Branch protection not working

**Symptom:** Can still push to main directly

**Solution:** Re-apply branch protection rules using the API command above.

### Issue: Documentation files missing

**Symptom:** CLAUDE.md or CODEX.md not found

**Solution:** Create them using the templates in this guide.

---

## Tips for Consistency

1. **Always start with this guide** when creating a new repository
2. **Copy-paste from existing repos** (ClipWave or Nautilus) and customize
3. **Test the workflow** by creating a dummy PR before real development
4. **Keep documentation updated** as the project evolves
5. **Reference this guide** in your CLAUDE.md for consistency

---

## Summary

Setting up a repository with SDD structure requires:

1. ✅ Create repository on GitHub
2. ✅ Use `main` as default branch
3. ✅ Enable branch protection
4. ✅ Create CLAUDE.md (development protocols)
5. ✅ Create CODEX.md (quick reference)
6. ✅ Update README.md (project overview)
7. ✅ Add `.claude/settings.local.json` to .gitignore
8. ✅ Test feature branch workflow
9. ✅ Create initial PR for documentation

**Time estimate:** 15-30 minutes per repository

**Benefits:**
- Consistent structure across all projects
- Clear development guidelines
- Optimized for AI collaboration
- Easy onboarding for new developers
- Professional documentation

---

## Reference Repositories

- **Nautilus:** https://github.com/franvillanu/nautilus
- **ClipWave:** https://github.com/franvillanu/clip-wave

Use these as templates when setting up new repositories.

---

*Last updated: 2026-01-02*
*Maintained by: Francisco Villanueva (@franvillanu)*
