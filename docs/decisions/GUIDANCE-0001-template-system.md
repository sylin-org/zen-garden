# GUIDANCE-0001: Guidance Template System

**Status:** Accepted
**Date:** 2026-02-01
**Author:** Infrastructure Team

## Context

Offering guidance files (`.guidance.md`) provide post-installation instructions to users. These instructions often include commands that reference dynamic values like IP addresses, ports, and hostnames.

### Problems with Current Approach

1. **Verbose commands** - Users see multi-line scripts to discover values we already know
2. **No conditional content** - Same guidance shown regardless of context (static IP vs DHCP)
3. **Copy-paste friction** - Commands require user modification before execution

### Example of Problematic Guidance

```bash
# Get IP first
PIHOLE_IP=$(getent hosts {{server-name}} | awk '{print $1}')
echo "nameserver $PIHOLE_IP" | sudo tee /etc/resolv.conf
```

When we *know* the static IP is `192.168.1.240`, this should simply be:

```bash
echo "nameserver 192.168.1.240" | sudo tee /etc/resolv.conf
```

## Decision

Implement a simple template engine with:
1. **Variable substitution** with fallback defaults
2. **Conditional blocks** for context-aware content
3. **Re-rendering on state changes** (e.g., when static IP is assigned)

### Template Syntax

**Variables:**
```
{{variable}}              # Simple substitution (empty string if not set)
{{variable|default}}      # Fallback value if variable is empty
```

**Conditionals:**
```
{{#if variable}}...{{/if}}           # Show if variable is set and truthy
{{#if !variable}}...{{/if}}          # Show if variable is NOT set or empty
{{#if variable=value}}...{{/if}}     # Show if variable equals value
{{#if variable}}...{{#else}}...{{/if}}  # If-else block
```

**Nesting:**
```
{{#if static-ip}}
  {{#if os=linux}}
    sudo resolvectl dns eth0 {{static-ip}}
  {{/if}}
{{/if}}
```

### Available Variables

| Variable | Type | Description | Example |
|----------|------|-------------|---------|
| `server-name` | String | Stone hostname | `stone-pearl-harbor` |
| `name` | String | Container/service name | `pihole` |
| `static-ip` | String | Assigned static IP (empty if DHCP) | `192.168.1.240` |
| `port` | Number | Default service port | `53` |
| `admin-port` | Number | Admin port (if named in manifest) | `8080` |
| `os` | String | Host operating system | `linux`, `windows`, `macos` |
| `arch` | String | CPU architecture | `x86_64`, `aarch64` |

### Design Principles

1. **One-liner commands** - If we know a value, embed it directly
2. **Minimal cognitive load** - Show only what's relevant to the user's context
3. **Copy-paste ready** - Commands should work without modification
4. **Progressive disclosure** - Essential info first, details in conditional blocks

## Consequences

### Positive

- **Better UX** - Commands are immediately usable
- **Context-aware** - Content adapts to deployment state
- **Maintainable** - Single source file handles multiple scenarios
- **Extensible** - Easy to add new variables as needed

### Negative

- **Template complexity** - Authors must learn syntax (mitigated by guidelines)
- **Re-rendering needed** - Guidance must update when state changes
- **Testing** - Need to verify both branches of conditionals

## Implementation

- Template engine: `src/common/src/templates.rs`
- Integration: `src/moss/src/tasks/job_executors.rs` (guidance rendering)
- Guidelines: `docs/guides/guidance-authoring.md`
- Golden standard: `src/moss/embedded/manifests/sw/networking/pihole.guidance.md`

## Alternatives Considered

### 1. Full template engine (Handlebars, Tera)

**Rejected:** Over-engineered for our needs. We don't need loops, helpers, or partials.

### 2. No conditionals, just variables

**Rejected:** Can't handle static-IP-vs-DHCP scenarios cleanly.

### 3. Multiple guidance files per offering

**Rejected:** Maintenance burden, content duplication.

## References

- [Mustache](https://mustache.github.io/) - Inspiration for syntax
- [Pi-hole guidance](../embedded/manifests/sw/networking/pihole.guidance.md) - Golden standard
