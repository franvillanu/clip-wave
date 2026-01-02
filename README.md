# ClipWave – Video Trimming Application

**ClipWave** is a simple, fast desktop video trimmer designed for quick edits. Trim your videos with precision using an intuitive interface.

![ClipWave Logo](Clipwave_logo_1.png)

## Features

- **Fast Video Trimming** - Quickly select start and end points
- **Video Preview** - Real-time playback and scrubbing
- **Multiple Formats** - Support for MP4, MOV, AVI and more
- **Desktop Native** - Built with Tauri for performance and small bundle size
- **Cross-Platform** - Windows, macOS, and Linux support
- **Modern UI** - Clean, responsive interface

## Tech Stack

- **Frontend:** React 19 + Vite
- **Desktop Framework:** Tauri 2.x
- **Language:** JavaScript (JSX)
- **Styling:** CSS3 with CSS Variables
- **Build Tool:** Vite

## Quick Start

### Prerequisites

- Node.js 18+ and npm
- Rust and Cargo (for Tauri)

### Installation

```bash
# Clone the repository
git clone https://github.com/franvillanu/clip-wave.git
cd clip-wave

# Install dependencies
npm install

# Start development server
npm run start
```

### Development Commands

```bash
npm run start      # Start Tauri dev mode (recommended)
npm run dev        # Start Vite dev server only
npm run build      # Build web assets
npm run tauri build # Build desktop installer
npm run lint       # Run ESLint
```

## Project Structure

```
clip-wave/
├── src/                    # React application source
│   ├── App.jsx            # Main application component
│   ├── main.jsx           # React entry point
│   └── styles/            # CSS stylesheets
├── src-tauri/             # Tauri Rust backend
│   ├── src/main.rs        # Rust main file
│   └── Cargo.toml         # Rust dependencies
├── public/                # Static assets
├── CLAUDE.md              # Development protocols for AI
├── CODEX.md               # Quick reference guide
└── README.md              # This file
```

## Development Philosophy: Specs-Driven Development (SDD)

ClipWave follows **Specs-Driven Development**, emphasizing comprehensive documentation over scattered code comments. This approach enables:

- **Efficient Development** - Clear guidelines reduce decision fatigue
- **AI Collaboration** - Token-efficient protocols for Claude Code
- **Consistency** - Standardized patterns across the codebase
- **Onboarding** - New developers understand the project quickly

### Key Documentation

| File | Purpose |
|------|---------|
| `README.md` | Project overview, setup, and features (this file) |
| `CLAUDE.md` | Token-efficient development protocols for Claude Code |
| `CODEX.md` | Quick reference guide for development |

## Git Workflow

ClipWave uses a **branch-based workflow** with protected main branch:

1. **Main branch is protected** - No direct commits allowed
2. **Feature branches** for all changes:
   - `feature/description` - New features
   - `fix/bug-name` - Bug fixes
   - `refactor/what` - Code refactoring
   - `docs/what` - Documentation updates
3. **Pull requests** for all merges
4. **Descriptive commits** with details

### Example Workflow

```bash
# Check current branch
git branch --show-current

# Create feature branch
git checkout -b feature/add-export-formats

# Make changes and commit
git add .
git commit -m "Add support for WebM export format"

# Push and create PR
git push -u origin feature/add-export-formats
# Then create PR via GitHub UI
```

## Building for Production

### Development Build

```bash
npm run build
```

### Production Installer

```bash
npm run tauri build
```

**Output locations:**
- **Windows:** `src-tauri/target/release/bundle/msi/`
- **macOS:** `src-tauri/target/release/bundle/dmg/`
- **Linux:** `src-tauri/target/release/bundle/appimage/`

## Architecture Overview

**Frontend (React):**
- Component-based architecture
- CSS variables for theming
- Vite for fast dev/build

**Backend (Tauri/Rust):**
- Native system integration
- File system access
- Video processing commands

**Communication:**
- React → Rust via `invoke()` commands
- Rust → React via events (when needed)

## Contributing

1. Fork the repository
2. Create a feature branch (`feature/amazing-feature`)
3. Make your changes following existing patterns
4. Test thoroughly with real video files
5. Commit with descriptive messages
6. Push to your fork and create a Pull Request

**Before submitting:**
- [ ] Code follows existing patterns
- [ ] ESLint passes (`npm run lint`)
- [ ] Tested with video files
- [ ] Commit messages are descriptive
- [ ] Documentation updated if needed

## Development Best Practices

### DO:
✓ Follow the Git workflow (feature branches + PRs)
✓ Use CSS variables for styling
✓ Handle errors in Tauri commands
✓ Test with actual video files
✓ Keep components focused and single-purpose
✓ Check `CODEX.md` for patterns before coding

### DON'T:
✗ Commit directly to main branch
✗ Hardcode values (use CSS variables/config)
✗ Skip error handling
✗ Ignore ESLint warnings
✗ Create large, monolithic components

## Resources

- [Tauri Documentation](https://tauri.app/)
- [React Documentation](https://react.dev/)
- [Vite Documentation](https://vitejs.dev/)

## License

This project is open source and available under the MIT License.

## Contact

**Author:** Francisco Villanueva ([@franvillanu](https://github.com/franvillanu))

**Repository:** [github.com/franvillanu/clip-wave](https://github.com/franvillanu/clip-wave)

---

Built with ❤️ using React + Tauri
