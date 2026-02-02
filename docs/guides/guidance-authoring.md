# Guidance Authoring Guide

This guide explains how to write effective `.guidance.md` files for offerings.

## Purpose

Guidance files provide post-installation instructions shown to users after an offering is deployed. Good guidance is:

- **Actionable** - Commands work immediately without modification
- **Context-aware** - Content adapts to the deployment state
- **Minimal** - Only essential information, no tutorials

## File Structure

```markdown
---
version: "1"
trigger: post_install
---
# Offering Name

Essential info here...
```

**Important:** Use exactly ONE `#` title (H1). This title is extracted and displayed as the collapsible panel header in the UI. Use `##` for all other sections.

### Frontmatter

| Field | Required | Values | Description |
|-------|----------|--------|-------------|
| `version` | Yes | `"1"` | Schema version |
| `trigger` | Yes | `post_install` | When to show guidance |

## Template Syntax

### Variables

```markdown
{{variable}}              # Substitution (empty if not set)
{{variable|default}}      # Substitution with fallback
```

### Available Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `server-name` | Stone hostname | `stone-pearl-harbor` |
| `name` | Container name | `pihole` |
| `static-ip` | Static IP (empty if DHCP) | `192.168.1.240` |
| `port` | Default service port | `53` |
| `admin-port` | Admin port (named port) | `8080` |
| `os` | Host OS | `linux`, `windows`, `macos` |
| `arch` | CPU architecture | `x86_64`, `aarch64` |

### Conditionals

```markdown
{{#if variable}}
  Shown if variable is set and non-empty
{{/if}}

{{#if !variable}}
  Shown if variable is NOT set or empty
{{/if}}

{{#if variable=value}}
  Shown if variable equals value
{{/if}}

{{#if variable}}
  If branch
{{#else}}
  Else branch
{{/if}}
```

### Nesting

Conditionals can be nested:

```markdown
{{#if static-ip}}
  {{#if os=linux}}
    sudo resolvectl dns eth0 {{static-ip}}
  {{/if}}
{{/if}}
```

## Supported Markdown

The guidance renderer supports a **subset** of markdown. Use only these elements:

**Supported:**
- Title: `#` (exactly one, becomes collapsible header)
- Section headers: `##`, `###`
- Bold: `**text**`
- Inline code: `` `text` `` (for short values, no copy button)
- Fenced code blocks: ``` ``` ``` (for commands, **with copy button**)
- Bullet lists: `- item`
- Links: `[text](url)`
- Horizontal rules: `---`

**Copy Button:** Only fenced code blocks get a copy button. Use inline code for short values like IPs or passwords. Use fenced code blocks for commands users need to copy and run.

**NOT Supported:**
- Tables (`| col |`)
- Numbered lists (`1.`)
- Images
- Blockquotes (`>`)
- HTML tags

Use bold labels followed by fenced code blocks:
```markdown
**Linux:**
```
echo "nameserver 1.2.3.4" | sudo tee /etc/resolv.conf
```

**macOS:**
```
sudo networksetup -setdnsservers Wi-Fi 1.2.3.4
```
```

## Writing Principles

### 1. One-liner Commands

If you know a value, embed it directly:

**Bad:**
```markdown
# Get the IP first
PIHOLE_IP=$(getent hosts {{server-name}} | awk '{print $1}')
echo "nameserver $PIHOLE_IP" | sudo tee /etc/resolv.conf
```

**Good:**
```markdown
{{#if static-ip}}
echo "nameserver {{static-ip}}" | sudo tee /etc/resolv.conf
{{/if}}
```

### 2. Essential Info First

Lead with what users need immediately:

```markdown
# Pi-hole

**Admin:** http://{{server-name}}:{{admin-port}}/admin
**Password:** `pihole`
```

### 3. Context-Aware Content

Don't show irrelevant information:

```markdown
{{#if static-ip}}
Set your router's DNS to: `{{static-ip}}`
{{#else}}
Get this stone's IP: `ping {{server-name}}`
{{/if}}
```

### 4. Copyable Code Blocks

Use fenced code blocks for commands (they get a copy button):

```markdown
**Change Password:**
```
docker exec -it {{name}} pihole -a -p
```
```

Use inline code for short values (no copy button needed):

```markdown
**Password:** `pihole`
**DNS:** `{{static-ip}}`
```

## Anti-Patterns

### Don't: Explain how the software works

Users installing Pi-hole know what DNS is.

### Don't: Show all OS variants

Use conditionals or just show the most common case.

### Don't: Include troubleshooting for common issues

Keep guidance focused on getting started.

### Don't: Duplicate documentation

Link to official docs instead.

## Golden Standard: Pi-hole

See [pihole.guidance.md](../../src/moss/embedded/manifests/sw/networking/pihole.guidance.md) for the complete example.

Key patterns demonstrated:
- Fenced code blocks for copyable commands
- `{{#if static-ip}}` conditional for context-aware content
- Essential info (admin URL, password) at the top
- Bold labels before each code block

## Checklist

Before submitting guidance:

- [ ] Exactly one `#` title (becomes collapsible header)
- [ ] Fenced code blocks for commands (enables copy button)
- [ ] Inline code only for short values (IPs, passwords)
- [ ] Commands are copy-paste ready (no placeholders to fill)
- [ ] Essential info (URL, credentials) is at the top
- [ ] Static IP vs DHCP cases handled with conditionals
- [ ] No tutorials or explanations of how the software works
- [ ] Only supported markdown used (no tables, numbered lists, images)
- [ ] Tested with both static IP and DHCP scenarios
