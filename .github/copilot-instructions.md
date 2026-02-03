# GitHub Copilot Instructions for Zen Garden

## Agnostic Context (Primary)

**Bootstrap from**: @../.agentic/CONTEXT.md

This file contains tool-agnostic rules shared across all AI assistants.

## Domain-Specific Rules (Load as needed)

- API development: @../.agentic/rules/api-handlers.md
- Docker operations: @../.agentic/rules/docker-ops.md
- Networking/P2P: @../.agentic/rules/networking.md
- Companions: @../.agentic/rules/companions.md
- Stone SSH: @../.agentic/rules/stone-ssh.md

## Reference (Don't reinvent)

- Utilities & constants: @../.agentic/reference/utilities.md
- API endpoints: @../.agentic/reference/api-endpoints.md

---

## Copilot-Specific: Changelog Maintenance

**File**: `docs/CHANGELOG.md` (single source of truth)

### Add Entry For
- New features, breaking changes, architectural refactorings
- User-visible bug fixes, security fixes

### Skip Entry For
- Typos, formatting, internal refactoring, test-only changes

### On Commit
1. If significant change → update `docs/CHANGELOG.md`
2. Add to date section: `## YYYY-MM-DD`
3. One-liner format: `- Description (under 120 chars)`

**Format**:
```markdown
## 2026-01-26
- Fixed syntax error in delete_service_v1()
- **BREAKING**: Renamed GARDEN_STONE_URL to GARDEN_STONE_ENDPOINT
```
