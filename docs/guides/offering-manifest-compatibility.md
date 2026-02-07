---
audience: [developer, contributor]
doc_type: guide
status: current
last_verified: 2026-01-31
canonical: true
---

# Manifest Compatibility & Port Handling Guide

This guide explains how to configure offering manifests for OS/platform compatibility and how the well-known ports catalog handles port conflicts automatically.

---

## Overview

Zen Garden provides two mechanisms to ensure offerings deploy successfully:

1. **Compatibility Rules** - Pre-flight checks that prevent deployment on incompatible platforms
2. **Well-Known Ports Catalog** - Automatic port conflict resolution during service installation

---

## OS/Platform Compatibility Rules

### When to Use

Add OS compatibility rules when an offering:
- Requires specific kernel features (e.g., Pi-hole needs proper DNS port binding)
- Uses platform-specific Docker features not available everywhere
- Has known issues on certain operating systems

### File Location

Compatibility rules live in `<offering>.compatibility.yaml` alongside the service snippet:

```
manifests/
└── networking/
    ├── pihole.snippet.yaml
    └── pihole.compatibility.yaml    # Compatibility rules here
```

### Schema

```yaml
version: "1"

compatibility_rules:
  - name: "rule-identifier"
    condition:
      # OS inclusion (match if OS is in list)
      os_family: ["linux", "macos"]

      # OS exclusion (match if OS is NOT in list)
      os_family_not: ["linux", "macos"]

      # Other conditions (can be combined)
      architectures: ["x86_64", "aarch64"]
      memory_mb_less_than: 256
      cpu_features_missing: ["avx"]

    reason: "Human-readable explanation"
    suggestion: "Actionable recommendation"
    warn_only: false  # Optional: true = warning only, false = block
```

### OS Family Values

| Value | Platform |
|-------|----------|
| `linux` | Linux (all distributions) |
| `macos` | macOS / Darwin |
| `windows` | Windows |

### Example: Block Windows Deployment

```yaml
# pihole.compatibility.yaml
version: "1"

compatibility_rules:
  - name: "windows-not-supported"
    condition:
      os_family_not: ["linux", "macos"]
    reason: "Pi-hole requires Linux or macOS for proper DNS port binding"
    suggestion: "Deploy Pi-hole on a Linux Stone (recommended) or use WSL2 with proper networking"
```

This rule **triggers** when the OS is NOT linux or macos (i.e., triggers on Windows).

### Example: Require Specific OS

```yaml
# some-linux-only-service.compatibility.yaml
version: "1"

compatibility_rules:
  - name: "linux-only"
    condition:
      os_family: ["linux"]
    reason: "This service requires Linux kernel features"
    suggestion: "Deploy on a Linux Stone"
```

Wait - this is inverted! The `os_family` condition **matches** when the OS IS in the list. So this rule would trigger ON linux, which is wrong.

For "require linux", use `os_family_not`:

```yaml
compatibility_rules:
  - name: "requires-linux"
    condition:
      os_family_not: ["linux"]
    reason: "This service requires Linux"
    suggestion: "Deploy on a Linux Stone"
```

### Condition Logic

- **`os_family: [...]`** - Rule triggers if current OS IS in the list
- **`os_family_not: [...]`** - Rule triggers if current OS is NOT in the list
- Multiple conditions are ANDed together
- Rules are evaluated in order; first match wins

### Combined Conditions

```yaml
compatibility_rules:
  - name: "arm32v6-not-supported"
    condition:
      architectures: ["armv6l"]
    reason: "Docker image requires ARMv7 or newer"
    suggestion: "Use Raspberry Pi 2 or newer"

  - name: "insufficient-memory"
    condition:
      memory_mb_less_than: 128
    reason: "Service requires at least 128MB RAM"
    suggestion: "Increase stone memory allocation"
```

---

## Well-Known Ports Catalog

The ports catalog provides automatic port conflict resolution during service installation.

### Location

```
manifests/well-known-ports.yaml           # Runtime overlay
src/moss/embedded/manifests/well-known-ports.yaml  # Embedded default
```

### Remediation Types

| Type | Behavior |
|------|----------|
| `remap` | Find next available port in specified range |
| `auto` | Run commands to free the port (platform-specific) |
| `manual` | Show message, user must resolve |
| `fail` | Cannot remediate, deployment fails |

### Catalog Schema

```yaml
version: "1"

ports:
  # Port number as key
  5432:
    name: postgresql
    description: "PostgreSQL database"

    # Cross-platform default (used when no platform-specific handler)
    default:
      type: remap
      range_start: 5433
      range_end: 5499

    # Platform-specific handlers (optional, override default)
    linux:
      common_culprit: "local postgresql"
      detection: "systemctl is-active --quiet postgresql"
      remediation:
        type: auto
        commands:
          - "systemctl stop postgresql"

    windows:
      common_culprit: "PostgreSQL Windows Service"
      remediation:
        type: manual
        message: "Stop PostgreSQL service in Services.msc"
```

### Remap Remediation (Most Common)

For regular services, use `remap` as the default. When the requested port is in use, Moss automatically finds the next available port in the specified range.

```yaml
ports:
  5432:
    name: postgresql
    description: "PostgreSQL database"
    default:
      type: remap
      range_start: 5433
      range_end: 5499
```

**Behavior:**
1. Service requests port 5432
2. Port 5432 is in use
3. Moss scans 5433, 5434, ... until finding an available port
4. Service deploys on the available port (e.g., 5433)

### Auto Remediation (Platform-Specific)

For essential ports that cannot be remapped (e.g., DNS must be on port 53), use platform-specific `auto` remediation.

```yaml
ports:
  53:
    name: dns
    description: "DNS queries - must be on port 53"
    linux:
      common_culprit: "systemd-resolved"
      detection: "systemctl is-active --quiet systemd-resolved"
      remediation:
        type: auto
        commands:
          - "systemctl disable --now systemd-resolved"
        files:
          - path: "/etc/resolv.conf"
            content: |
              # Configured by Zen Garden
              nameserver 8.8.8.8
              nameserver 1.1.1.1
    macos:
      common_culprit: "mDNSResponder"
      remediation:
        type: manual
        message: "Port 53 is in use by mDNSResponder. Check with: sudo lsof -i :53"
    windows:
      common_culprit: "DNS Client service"
      remediation:
        type: fail
        message: "DNS services are not supported on Windows. Deploy on a Linux Stone instead."
```

**Behavior (Linux):**
1. Pi-hole requests port 53
2. Port 53 is in use
3. Detection command confirms systemd-resolved is the culprit
4. Auto commands run to disable systemd-resolved
5. `/etc/resolv.conf` is created with fallback DNS
6. Port 53 becomes available
7. Pi-hole deploys on port 53

### Adding a New Port

When adding a new offering that uses a well-known port:

1. Check if the port already exists in the catalog
2. If not, add it with sensible defaults:

```yaml
ports:
  # Your new port
  9042:
    name: cassandra
    description: "Apache Cassandra CQL"
    default:
      type: remap
      range_start: 9043
      range_end: 9099
```

### Port Resolution Flow

```
1. Service requests port P
2. Is port P available?
   └─ Yes → Use port P
   └─ No → Check catalog
            ├─ Has platform handler?
            │   └─ Yes → Execute handler (auto/manual/fail)
            │            └─ Success → Use port P
            ├─ Has default remediation?
            │   └─ Remap → Find available port in range
            │   └─ Auto → Execute commands
            │   └─ Manual/Fail → Show message, fail
            └─ No catalog entry → Generic error with diagnostic commands
```

---

## Best Practices

### For Compatibility Rules

1. **Be specific** - Name rules clearly (e.g., `windows-not-supported`, `arm32v6-not-supported`)
2. **Provide suggestions** - Tell users what to do, not just what's wrong
3. **Use `warn_only: true`** for soft constraints (performance warnings, not hard failures)
4. **Test on target platforms** - Verify rules trigger correctly

### For Port Catalog

1. **Prefer remap** - Most services work fine on alternate ports
2. **Use auto sparingly** - Only for essential ports that cannot be remapped
3. **Choose sensible ranges** - Start at port+1, allow ~50-100 ports in range
4. **Document platform differences** - If Windows can't run a service, say so explicitly

### Manifest Structure Summary

```
manifests/
├── well-known-ports.yaml                    # Port conflict catalog
└── <category>/
    ├── <offering>.snippet.yaml              # Docker Compose definition
    └── <offering>.compatibility.yaml        # Optional: compatibility rules
```

---

## Reference: Current Port Catalog

| Port | Service | Default Remediation |
|------|---------|-------------------|
| 53 | DNS | Platform-specific (auto on Linux, fail on Windows) |
| 80 | HTTP | Remap to 8080-8099 |
| 443 | HTTPS | Remap to 8443-8499 |
| 3000 | Grafana | Remap to 3001-3099 |
| 3306 | MySQL/MariaDB | Remap to 3307-3399 |
| 4222 | NATS | Remap to 4223-4299 |
| 5432 | PostgreSQL | Remap to 5433-5499 |
| 5672 | RabbitMQ | Remap to 5673-5699 |
| 6379 | Redis | Remap to 6380-6399 |
| 9000 | MinIO | Remap to 9001-9099 |
| 9090 | Prometheus | Remap to 9091-9099 |
| 11211 | Memcached | Remap to 11212-11299 |
| 11434 | Ollama | Remap to 11435-11499 |
| 27017 | MongoDB | Remap to 27018-27099 |

---

## Troubleshooting

### Rule Not Triggering

1. Check condition logic - `os_family` matches ON the list, `os_family_not` matches when NOT on list
2. Verify YAML syntax - conditions must be valid
3. Check rule order - first matching rule wins

### Port Still Conflicting

1. Verify port is in catalog
2. Check platform-specific handler exists for your OS
3. For `auto` remediation, verify detection command returns correct result
4. Check remap range isn't exhausted

### Service Deployed on Wrong Port

1. Check logs for "Port remapped" messages
2. Use `rake status <offering>` to see actual port bindings
3. Update connection strings to use actual port

---

## Offering Guidance (Post-Install Notes)

Guidance files provide post-installation notes displayed on the stone's portrait page (port 7185). These notes help users configure and use the offering after deployment.

### File Location

Guidance files live alongside the snippet as `<offering>.guidance.md`:

```
manifests/
└── networking/
    ├── pihole.snippet.yaml
    ├── pihole.compatibility.yaml
    └── pihole.guidance.md          # Guidance notes here
```

### Format

Guidance files use Markdown with an optional YAML frontmatter:

```markdown
---
version: "1"
trigger: post_install
---
# Offering Title

Your post-installation notes here.
```

### Frontmatter Fields

| Field | Values | Description |
|-------|--------|-------------|
| `version` | `"1"` | Schema version (always "1") |
| `trigger` | `post_install` (default), `always` | When to show notes |

### Template Variables

Use template variables that get replaced with actual values after installation:

| Variable | Description | Example |
|----------|-------------|---------|
| `{{port}}` | The service's native port | `8053` |
| `{{server-name}}` | The stone's hostname | `stone-01` |
| `{{offering}}` | The offering type | `pihole` |
| `{{name}}` | The service instance name | `pihole` |

**Example usage:**

```markdown
Open the web interface at:

```
http://{{server-name}}:{{port}}/admin
```
```

### Supported Markdown Subset

The portrait page uses a minimal markdown parser. Supported elements:

| Element | Syntax | Renders As |
|---------|--------|------------|
| Heading 1 | `# Title` | `<h1>Title</h1>` |
| Heading 2 | `## Title` | `<h2>Title</h2>` |
| Heading 3 | `### Title` | `<h3>Title</h3>` |
| Bold | `**text**` | `<strong>text</strong>` |
| Inline code | `` `code` `` | `<code>code</code>` |
| Code block | ` ```bash ... ``` ` | `<pre><code>...</code></pre>` |
| Unordered list | `- item` or `* item` | `<ul><li>item</li></ul>` |
| Link | `[text](url)` | `<a href="url">text</a>` |
| Paragraph | Blank line between text | `<p>...</p>` |

**Not supported:** Images, tables, blockquotes, horizontal rules, ordered lists, nested lists.

### Code Block Copy Button

Code blocks automatically get a "Copy" button. Users can click to copy the command to their clipboard - useful for shell commands.

### Example: Complete Guidance File

```markdown
---
version: "1"
trigger: post_install
---
# Pi-hole Configuration

Your Pi-hole DNS server is now running on **{{server-name}}**.

## Access the Admin Console

Open the web interface at:

```
http://{{server-name}}:{{port}}/admin
```

**Default password:** `pihole`

## Change the Admin Password

For security, change the default password:

```bash
docker exec -it {{name}} pihole -a -p
```
```

### Display Behavior

- Notes appear as a collapsible "Notes" section under each offering card
- Open by default on first view
- Collapse state is remembered in localStorage
- Only offerings with guidance files show the Notes section

---

## See Also

- [COMPAT-0001: Compatibility Rules System](../decisions/COMPAT-0001-compatibility.md)
- [Service Catalog](../reference/offerings.md)
- [manifests/README.md](../../manifests/README.md)
