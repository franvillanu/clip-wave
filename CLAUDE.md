# CLAUDE.md - ClipWave Development Protocol

## Role Profiles (Auto-Select) - Mandatory
If no role is specified, pick the best fit based on the request:
- Architect: planning, design, interfaces, risks, scope clarification.
- Implementer: code changes, feature work, bug fixes, tests.
- Reviewer: review diffs, find issues, give verdict.
- QA: test strategy, edge cases, fixtures, validation.

See `.sdd/README.md` for usage and templates.

This document establishes token-efficient development protocols for Claude working on the ClipWave video trimming application.

## Core Principles

**10x Token Reduction Strategy:**
- Never re-read cached files from current session
- Use Grep searches (~50-200 tokens) before reading entire files
- Read only necessary sections using offset/limit parameters
- Reference specs instead of reading full code files
- Edit existing code rather than rewriting files

## Project Overview

ClipWave is a desktop video trimming application built with:
- **Frontend:** React + TypeScript + Vite
- **Desktop Framework:** Tauri (Rust backend)
- **UI:** Modern, responsive interface for video editing

## Git Workflow (MANDATORY)

**Branch Protection:**
- Main branch is protected - NO direct commits allowed
- Always work on feature branches
- Users create pull requests for all changes

**Workflow Steps:**
1. Check current branch immediately (`git branch --show-current`)
2. If on main, create feature branch: `feature/description` or `fix/description`
3. Push branch before making changes
4. Make commits with descriptive messages following format:
   ```
   Brief description of change

   - Bullet point details
   - What was changed and why

   🤖 Generated with [Claude Code](https://claude.com/claude-code)

   Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>
   ```
5. **NEVER** merge to main - user creates PR via GitHub UI

## Code Modification Guidelines

**Before Making Changes:**
1. Use Grep to locate relevant code
2. Read only the specific files/sections needed
3. Understand existing patterns in the codebase
4. Edit existing code - don't rewrite unless necessary

**Quality Standards:**
- Maintain TypeScript type safety
- Follow existing code patterns and conventions
- Test changes when they affect core functionality
- Keep commits focused and atomic

## Token Optimization Hierarchy

1. **Use cached information** from current session
2. **Use Grep** to search code (~50-200 tokens)
3. **Read specific sections** with offset/limit
4. **Reference specs** when available
5. **Full file reads** only as last resort

## Supporting Documentation

- `README.md` - Project overview and setup
- `CODEX.md` - Quick reference guide for development
- Architecture docs (if created) - Technical design patterns

## Best Practices

**DO:**
- Check branch before any commits
- Use descriptive commit messages
- Edit existing files when possible
- Test video trimming functionality after changes
- Follow TypeScript best practices

**DON'T:**
- Commit directly to main
- Re-read files already in session cache
- Rewrite entire files for small changes
- Skip testing for core feature changes
- Hardcode values that should be configurable

## Emergency Fallback

If token limits are reached, refer to `CODEX.md` for essential project information that can be used with GitHub Copilot or ChatGPT to maintain consistency.
