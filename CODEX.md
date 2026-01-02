# CODEX.md - ClipWave Quick Reference Guide

This document serves as a comprehensive reference guide for development on ClipWave, a desktop video trimming application. Use this as a fallback when token limits are reached or for quick reference with GitHub Copilot or ChatGPT.

## Project Overview

**ClipWave** is a desktop application for trimming video files with a clean, modern interface.

**Tech Stack:**
- **Frontend:** React 19 (JSX), Vite
- **Desktop:** Tauri 2.x (Rust backend)
- **UI:** CSS3 with CSS Variables
- **Build:** Vite bundler

## Project Structure

```
clip-wave/
├── src/
│   ├── App.jsx              # Main application component
│   ├── main.jsx             # React entry point
│   ├── styles/
│   │   ├── app.css          # Application styles
│   │   ├── reset.css        # CSS reset
│   │   └── variables.css    # CSS custom properties
├── src-tauri/               # Tauri Rust backend
│   ├── src/
│   │   └── main.rs          # Rust main file
│   └── Cargo.toml           # Rust dependencies
├── public/                  # Static assets
├── index.html               # HTML entry point
├── package.json             # Node dependencies
├── vite.config.js           # Vite configuration
├── CLAUDE.md                # Development protocols
├── CODEX.md                 # This file
└── README.md                # Project documentation
```

## Development Commands

```bash
npm run start      # Start Tauri dev server (recommended)
npm run dev        # Start Vite dev server only
npm run build      # Build for production
npm run lint       # Run ESLint
npm run tauri      # Run Tauri CLI commands
```

## Git Workflow (MANDATORY)

**All development uses feature branches:**

Branch naming conventions:
- `feature/description` - New features
- `fix/bug-name` - Bug fixes
- `refactor/what` - Code refactoring
- `docs/what` - Documentation updates

**Workflow:**
1. Check current branch: `git branch --show-current`
2. If on main, create feature branch
3. Make changes and commit with descriptive messages
4. Push branch to origin
5. Create PR via GitHub UI (NEVER merge directly to main)

**Pre-commit hooks prevent direct commits to main.**

## Code Organization

### React Components (src/App.jsx)

**Component Structure:**
```jsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

function ComponentName() {
  // State declarations
  const [state, setState] = useState(initialValue);

  // Effects
  useEffect(() => {
    // Effect logic
  }, [dependencies]);

  // Event handlers
  const handleEvent = async () => {
    // Handler logic
  };

  // Render
  return (
    <div>
      {/* JSX */}
    </div>
  );
}

export default ComponentName;
```

### Tauri Integration

**Invoking Rust Commands:**
```jsx
import { invoke } from '@tauri-apps/api/core';

// Call Rust backend
const result = await invoke('command_name', {
  param: value
});
```

**File Dialogs:**
```jsx
import { open } from '@tauri-apps/plugin-dialog';

// Open file picker
const selected = await open({
  multiple: false,
  filters: [{
    name: 'Video',
    extensions: ['mp4', 'mov', 'avi']
  }]
});
```

### Styling Guidelines

**Use CSS Variables:**
```css
/* Define in variables.css */
:root {
  --primary-color: #007bff;
  --spacing-md: 16px;
}

/* Use in components */
.button {
  background: var(--primary-color);
  padding: var(--spacing-md);
}
```

**Naming Conventions:**
- Use kebab-case for CSS classes: `.video-player`, `.trim-controls`
- Prefix component-specific styles: `.app-header`, `.app-footer`
- Use BEM for complex components: `.block__element--modifier`

## Best Practices

### DO:
✓ Keep components focused and single-purpose
✓ Use async/await for Tauri commands
✓ Handle errors in Tauri invocations
✓ Use CSS variables for all colors/spacing
✓ Test video trimming functionality
✓ Follow existing code patterns
✓ Keep functions under 50 lines
✓ Add error handling for file operations

### DON'T:
✗ Commit directly to main branch
✗ Hardcode file paths or URLs
✗ Skip error handling on Tauri calls
✗ Use inline styles (use CSS classes)
✗ Create global state unnecessarily
✗ Ignore ESLint warnings
✗ Forget to test on actual video files

## Common Patterns

### Error Handling
```jsx
try {
  const result = await invoke('trim_video', {
    path: videoPath,
    start: startTime,
    end: endTime
  });
  console.log('Success:', result);
} catch (error) {
  console.error('Error trimming video:', error);
  // Show user-friendly error message
}
```

### State Management
```jsx
// Simple state for UI
const [isPlaying, setIsPlaying] = useState(false);
const [currentTime, setCurrentTime] = useState(0);

// Complex state - use object
const [videoState, setVideoState] = useState({
  path: null,
  duration: 0,
  trimStart: 0,
  trimEnd: 0
});

// Update specific property
setVideoState(prev => ({
  ...prev,
  trimStart: newValue
}));
```

## Tauri Backend (Rust)

**Command Definition (src-tauri/src/main.rs):**
```rust
#[tauri::command]
fn command_name(param: String) -> Result<String, String> {
    // Implementation
    Ok(result)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![command_name])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Testing Checklist

Before committing:
- [ ] Video file can be selected
- [ ] Video playback works
- [ ] Trim markers can be set
- [ ] Trim operation completes successfully
- [ ] Output file is created correctly
- [ ] Error states are handled gracefully
- [ ] UI is responsive
- [ ] No console errors

## Deployment

**Build for Production:**
```bash
npm run build          # Builds web assets
npm run tauri build    # Creates installer
```

**Output Locations:**
- Windows: `src-tauri/target/release/bundle/msi/`
- macOS: `src-tauri/target/release/bundle/dmg/`
- Linux: `src-tauri/target/release/bundle/appimage/`

## Quick Reference

| Task | Command/Pattern |
|------|----------------|
| Create feature branch | `git checkout -b feature/name` |
| Call Rust from React | `await invoke('command', { args })` |
| Open file dialog | `await open({ filters: [...] })` |
| Define CSS variable | `:root { --var-name: value; }` |
| Use CSS variable | `var(--var-name)` |
| Build for production | `npm run tauri build` |
| Start dev server | `npm run start` |

## Emergency Fallback

If you need to continue development without Claude Code:
1. Review this CODEX.md for project structure
2. Check CLAUDE.md for development protocols
3. Follow existing code patterns in src/App.jsx
4. Test all changes with actual video files
5. Create PR for review (never commit to main)

## Support Documentation

- `README.md` - Project overview and setup instructions
- `CLAUDE.md` - Token-efficient development protocols for Claude
- Tauri Docs: https://tauri.app/
- React Docs: https://react.dev/
