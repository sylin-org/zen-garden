---
audience: [developer, contributor]
doc_type: guide
status: current
last_verified: 2026-05-29
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

A compatibility file has two top-level sections, both optional:

```yaml
compatibility_rules:          # pre-flight checks, evaluated at plant time
  - name: "rule-identifier"   # required: stable identifier (appears in logs)
    when:                     # required: predicate-DSL expressions; ALL must match (AND)
      - host.architecture IN (aarch64,arm64,armv7l,armv6l)
    reason: "Human-readable explanation"        # required
    suggestion: "Actionable recommendation"     # optional
    warn_only: false          # optional (default false): true = warn, false = block
    continue: false           # optional (default false): with warn_only, keep evaluating later rules
    fallback:                 # optional: deploy a different image instead of failing
      image: "alt/image:tag"
      name: "legacy"          # optional: instance suffix → FQN becomes <offering>::legacy

post_install_healthcheck:     # post-deploy log scan (see "Post-Install Healthcheck" below)
  enabled: true
  scan_log_lines: 100
  timeout_seconds: 30
  patterns:
    - pattern: "Cannot allocate memory|OOM"   # regex matched against container logs
      reason: "Insufficient memory"
      suggestion: "Increase RAM allocation"
      fallback: { image: "alt/image:tag", name: "legacy" }   # optional
```

Each rule is `deny_unknown_fields` — an unrecognized key (including the legacy `condition:` block) is a hard parse error and the whole offering is skipped. All matching logic lives in `when:` (the predicate DSL), never in named condition keys. A top-level `version: "1"` is optional and ignored by the loader. `garden-rake manifest validate` parses every `when:` expression, so a malformed predicate is reported (`COMPAT003`) at authoring time rather than silently dropped at plant.

### The `when:` predicate DSL

Each `when:` entry is one expression of the form `fact OPERATOR value`. Entries within a rule are ANDed; express OR across facts by writing separate rules. (Grammar defined in [COMPAT-0002](../decisions/COMPAT-0002-predicate-dsl.md).)

**Facts** (the `host.*` namespace) and the operators each type accepts:

| Fact | Type | Operators |
|------|------|-----------|
| `host.architecture` | scalar | `IS`, `IS NOT`, `IN`, `NOT IN` |
| `host.os.family` | scalar | `IS`, `IS NOT`, `IN`, `NOT IN` |
| `host.cpu.model` | scalar | `IS`, `IS NOT`, `IN`, `NOT IN` |
| `host.cpu.pattern` | set | `HAS`, `HAS ALL`, `LACKS` |
| `host.cpu.features` | set | `HAS`, `HAS ALL`, `LACKS` |
| `host.ai.runtime` | set | `HAS`, `HAS ALL`, `LACKS` |
| `host.ram.total.mb` | numeric | `>=`, `>`, `<`, `<=` |
| `host.gpu.count` | numeric | `>=`, `>`, `<`, `<=` |
| `host.gpu.vram.total.mb` / `host.gpu.vram.total.gb` | numeric | `>=`, `>`, `<`, `<=` |
| `host.gpu` / `host.npu` | boolean | `IS present`, `IS NOT present` |

**Value syntax by type:**

- Scalar — `host.architecture IS armv6l`, `host.os.family NOT IN (linux,macos)`
- Set — `host.cpu.features LACKS avx`, `host.cpu.pattern HAS j4105,j3455` (comma = OR), `host.ai.runtime HAS cuda AND rocm` (all-of)
- Numeric — `host.ram.total.mb < 2048`
- Boolean — `host.gpu IS present`

Operators and facts are case-insensitive; values are lowercased on parse. Mixing `AND` and `OR` in one expression is rejected — split into separate `when:` entries. Using the wrong operator for a fact's type (e.g. `host.ram.total.mb HAS 4096`) is a parse error. **OS family values:** `linux`, `macos`, `windows`.

### How rules are evaluated

Rules are checked in order; the **first matching rule wins** and decides the outcome:

| Rule shape | Outcome when it matches |
|------------|-------------------------|
| no `fallback`, no `warn_only` | **Fail** — deployment is blocked with `reason` |
| `fallback: {image, name}` | **Fallback** — image swapped to `fallback.image`, GPU device requests cleared, FQN becomes `<offering>::<name>` |
| `warn_only: true` | **Warning** — deploys anyway; `continue: true` keeps evaluating later rules |

If no rule matches, the result is **Pass**. A `when:` expression that fails to parse is logged and that single rule is skipped.

### Example: block by OS

```yaml
# pihole.compatibility.yaml
compatibility_rules:
  - name: "windows-not-supported"
    when:
      - host.os.family NOT IN (linux,macos)
    reason: "Pi-hole requires Linux or macOS for proper DNS port binding"
    suggestion: "Deploy Pi-hole on a Linux Stone (recommended) or use WSL2 with proper networking"
```

### Example: deny an unsupported architecture

```yaml
# sqlserver.compatibility.yaml — the image is amd64-only
compatibility_rules:
  - name: "non-x86_64-unsupported"
    when:
      - host.architecture IN (aarch64,arm64,armv7l,armv6l)
    reason: "SQL Server Linux container images are supported only on x86_64 (amd64)"
    suggestion: "Use PostgreSQL or MongoDB on ARM, or move to x86_64 hardware"
```

### Example: RAM floor (hard) plus a soft warning

```yaml
compatibility_rules:
  - name: "insufficient-memory"
    when:
      - host.ram.total.mb < 128
    reason: "Service requires at least 128MB RAM"
    suggestion: "Increase stone memory allocation"

  - name: "low-memory-warning"
    when:
      - host.ram.total.mb < 256
    reason: "Performs better with 256MB+ RAM"
    suggestion: "Consider increasing RAM"
    warn_only: true
```

### Fallback images (per-host image swap)

Rather than failing on incompatible hardware, a rule can deploy a different image. When the rule matches, Moss rewrites the service image to `fallback.image`, clears any GPU device requests, and — if `name` is given — deploys under the instance FQN `<offering>::<name>`:

```yaml
# mongodb.compatibility.yaml — fall back to 4.4 on CPUs without AVX
compatibility_rules:
  - name: "missing-avx-feature"
    when:
      - host.cpu.features LACKS avx
    reason: "MongoDB 5.0+ requires AVX CPU support"
    fallback:
      image: "mongo:4.4"
      name: "legacy"
```

### Post-Install Healthcheck

The `post_install_healthcheck` block declares regex patterns to match against the container's logs after deployment — intended to catch runtime failures the pre-flight `when:` rules cannot predict (e.g. a CPU that advertises a feature but then crashes with "Illegal instruction"), optionally triggering the same `fallback` image swap:

```yaml
post_install_healthcheck:
  enabled: true
  scan_log_lines: 100
  timeout_seconds: 30
  patterns:
    - pattern: "Illegal instruction"
      reason: "CPU instruction set incompatibility"
      fallback: { image: "mongo:4.2", name: "legacy42" }
    - pattern: "Cannot allocate memory|OOM"
      reason: "Insufficient memory"
      suggestion: "Increase stone RAM or reduce service count"
```

> **Enforcement (COMPAT-0003):** after a deploy, Moss scans the container's recent logs against these patterns (off the hot path, so healthy deploys aren't delayed). A matching pattern with a `fallback` recreates the container on the fallback image — volumes preserved, at most once per deploy; a matching pattern without a fallback emits a warning. Pre-flight `compatibility_rules` are still evaluated first, at plant time.

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

1. Check the predicate - a `when:` entry only triggers when it evaluates true (e.g. `host.os.family NOT IN (linux,macos)` triggers on Windows). A reference to a fact the stone could not detect evaluates false.
2. Verify the operator matches the fact type (scalar/set/numeric/boolean) - a type mismatch is a parse error and skips the rule
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
- [COMPAT-0002: Compatibility Predicate DSL](../decisions/COMPAT-0002-predicate-dsl.md)
- [Service Catalog](../reference/offerings.md)
- [manifests/README.md](../../src/moss/embedded/manifests/README.md)
