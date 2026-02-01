# Named Ports in Offering Manifests

## Overview

All port mappings in offering manifests are named. This enables:
- Clear identification of each port's purpose
- Dynamic template substitution in guidance documents
- Consistent port referencing across the system

## Manifest Format

Ports are defined as a YAML map where keys are port names and values are `[host_port, container_port]` tuples:

```yaml
ports:
  default: [53, 53]
  admin: [8053, 80]
```

### Naming Convention

- `default` - The primary service port (required for most offerings)
- Other names should be descriptive: `admin`, `management`, `grpc`, `transport`, `console`, etc.

## Template Variables

Named ports map to template variables in guidance documents:

| Port Name | Template Variable |
|-----------|-------------------|
| `default` | `{{port}}` |
| `admin` | `{{admin-port}}` |
| `management` | `{{management-port}}` |
| `<name>` | `{{<name>-port}}` |

### Example: Pi-hole Guidance

```markdown
## Ports

| Service | Port | Purpose |
|---------|------|---------|
| DNS | {{port}} | DNS queries (TCP/UDP) |
| Web Admin | {{admin-port}} | Admin dashboard |

## Access the Admin Console

Open the web interface at:
http://{{server-name}}:{{admin-port}}/admin
```

## Code Structure

### Types

- `ServiceTemplate.ports: HashMap<String, (u16, u16)>` - Parsed template
- `CompiledOffering.ports: HashMap<String, (u16, u16)>` - Compiled for API/installation

### Helper Methods

Both `ServiceTemplate` and `CompiledOffering` provide:

```rust
// Get the default port tuple
fn default_port(&self) -> Option<&(u16, u16)>

// Get the default host port (for registry)
fn default_host_port(&self) -> u16

// Get all ports as Vec for Docker API
fn ports_vec(&self) -> Vec<(u16, u16)>
```

### Template Substitution

Located in `job_executors.rs` and `adoption.rs`:

```rust
fn substitute_guidance_templates(
    template: &str,
    name: &str,
    offering: &str,
    ports: &HashMap<String, (u16, u16)>,
    stone_name: &str,
) -> String
```

## Examples

### Single Port (MongoDB)

```yaml
ports:
  default: [27017, 27017]
```

### Multiple Ports (RabbitMQ)

```yaml
ports:
  default: [5672, 5672]      # AMQP protocol
  management: [15672, 15672]  # Management UI
```

### Remapped Port (Nextcloud)

```yaml
ports:
  default: [8080, 80]  # Host 8080 maps to container 80
```

## Migration

All manifests use the named format. The old array format is no longer supported:

```yaml
# OLD (not supported)
ports:
  - [5672, 5672]
  - [15672, 15672]

# NEW (required)
ports:
  default: [5672, 5672]
  management: [15672, 15672]
```
