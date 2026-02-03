# Agentic Context - Tool-Agnostic AI Rules

This directory contains tool-agnostic context for AI coding assistants (Claude, Cursor, Copilot, etc.).

## Structure

```
.agentic/
├── CONTEXT.md              # Root context (always loaded)
├── README.md               # This file
├── rules/                  # Domain-specific rules (loaded on-demand)
│   ├── api-handlers.md     # API endpoint patterns
│   ├── companions.md       # Companion framework rules
│   ├── docker-ops.md       # Container operations
│   └── networking.md       # P2P transport rules
└── reference/              # Lookup tables (don't reinvent)
    ├── api-endpoints.md    # All REST endpoints
    └── utilities.md        # Existing functions & constants
```

## How It Works

Tool-specific configurations bootstrap from this directory:

| Tool | Config File | 
|------|-------------|
| **GitHub Copilot** | `.github/copilot-instructions.md` |
| **Claude Code** | `CLAUDE.md` |
| **Cursor** | `.cursorrules` |
| **Windsurf** | `.windsurfrules` |
| **Cline** | `.clinerules` |
| **Aider** | `CONVENTIONS.md` |
| **Cody (Sourcegraph)** | `.sourcegraph/cody.md` |
| **CodeGPT** | `.codegpt/instructions.md` |
| **Amazon Q** | `.amazonq/rules.md` |

All bootstrappers point to this `.agentic/` directory as the single source of truth.

## Adding New Rules

Create a new file in `rules/` with frontmatter:

```markdown
---
globs: path/to/affected/**/*.rs
alwaysApply: false
---
# Rule Name

[Your rules here]
```

The `globs` pattern helps AI assistants understand when to apply the rules.

## Maintenance

- Keep `CONTEXT.md` concise (<100 lines of actionable rules)
- Move detailed documentation to `docs/ARCHITECTURE-REFERENCE.md`
- Add domain-specific rules to `rules/` subdirectory
- Review periodically: if AI already does something correctly, remove the rule
