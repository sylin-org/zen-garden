# Harbor Registry Research

## Overview

| Property | Value |
|----------|-------|
| **Official Name** | Harbor |
| **Category** | Container Registry |
| **Primary Use** | Enterprise container registry with security |
| **License** | Apache 2.0 |
| **Governance** | CNCF Graduated |
| **Project URL** | https://goharbor.io/ |
| **GitHub** | https://github.com/goharbor/harbor |
| **Runtime** | Go + React |

## Why Harbor?

Harbor is the enterprise choice for container registries:
- **CNCF Graduated** - production-ready, widely adopted
- **Vulnerability Scanning** - Trivy/Clair integration
- **RBAC** - Role-based access control
- **Replication** - Multi-site registry sync
- **Artifact Signing** - Cosign/Notary support
- **Web UI** - Full management interface

## Important Note

**Harbor is a complex multi-container application.**

Unlike simple registries (Docker Registry, Zot), Harbor requires:
- PostgreSQL database
- Redis cache
- Multiple Harbor services (core, portal, registry, jobservice)

The snippet.yaml provided is for **infrastructure handler recognition only**.
For actual deployment, use Harbor's official installer.

## Docker Image Analysis

### Architecture Support

| Architecture | Supported | Notes |
|--------------|-----------|-------|
| amd64 | Yes | Primary platform |
| arm64v8 | Yes | Since Harbor 2.5 |
| arm32v7 | **No** | Not supported |
| arm32v6 | **No** | Not supported |

**ARM32 devices cannot run Harbor** - use Zot or Docker Registry instead.

## Resource Requirements

### Memory

Harbor is resource-intensive:

| Component | Memory | Notes |
|-----------|--------|-------|
| Harbor Core | 256MB | API server |
| Portal | 128MB | Web UI |
| Registry | 256MB | Image storage |
| Database | 256MB | PostgreSQL |
| Redis | 128MB | Cache |
| **Total** | **1GB+** | Minimum recommended |

For production, allocate 2-4GB.

### CPU

| Deployment | Cores |
|------------|-------|
| Minimum | 2 |
| Recommended | 4 |
| Production | 4+ |

### Disk

| Use Case | Storage |
|----------|---------|
| Small | 50GB |
| Medium | 200GB-1TB |
| Enterprise | Multi-TB |

## Full Deployment

### Using Harbor Installer

```bash
# Download Harbor installer
wget https://github.com/goharbor/harbor/releases/download/v2.11.2/harbor-offline-installer-v2.11.2.tgz

# Extract
tar xzf harbor-offline-installer-v2.11.2.tgz
cd harbor

# Configure
cp harbor.yml.tmpl harbor.yml
# Edit harbor.yml with your settings

# Install
./install.sh
```

### Minimum harbor.yml

```yaml
hostname: registry.local
http:
  port: 8080
harbor_admin_password: Harbor12345
database:
  password: root123
  max_idle_conns: 50
  max_open_conns: 100
data_volume: /data
```

## Network Configuration

| Port | Protocol | Purpose |
|------|----------|---------|
| 80/443 | HTTP/HTTPS | Web UI and API |
| 4443 | HTTPS | Notary (optional) |

Internal ports for multi-container communication.

## Key Features

### Vulnerability Scanning
```bash
# Enable in harbor.yml
trivy:
  enabled: true
```

### Replication
Sync images between Harbor instances or external registries.

### Robot Accounts
Service accounts for CI/CD pipelines.

### Garbage Collection
Automated image cleanup with configurable policies.

## Comparison

| Feature | Harbor | Zot | Docker Registry |
|---------|--------|-----|-----------------|
| License | Apache 2.0 | Apache 2.0 | Apache 2.0 |
| Complexity | High | Low | Low |
| Memory | 1GB+ | 64MB | 64MB |
| Web UI | Yes | Optional | No |
| Scanning | Yes | No | No |
| RBAC | Yes | Basic | No |
| ARM32 | No | Yes | Yes |
| CNCF | Graduated | No | Incubating |

## Raspberry Pi Compatibility

| Device | Support | Notes |
|--------|---------|-------|
| Pi 5 (8GB) | Yes | ARM64, sufficient RAM |
| Pi 4 (4GB+) | Marginal | May run but tight |
| Pi 4 (2GB) | No | Insufficient RAM |
| Pi 3/2/Zero | No | ARM32 not supported |

**Recommendation**: Use Zot or Docker Registry for Raspberry Pi deployments.

## Infrastructure Handler Integration

Despite being multi-container, Harbor is recognized by the Docker registry
infrastructure handler via:
- Name match: "harbor"
- Category: "devops"
- Tag: "container-registry"

When Harbor is deployed (manually or via compose), other Stones will
automatically add it to their Docker daemon's insecure-registries.

## Security Considerations

| Concern | Mitigation |
|---------|------------|
| Default password | Change harbor_admin_password immediately |
| HTTP default | Configure HTTPS in production |
| Database access | Restrict PostgreSQL access |

## Validation Checklist

- [x] Docker images exist (goharbor/* official images)
- [x] Architecture support verified (amd64, arm64 only)
- [x] Memory constraints documented (1GB+ minimum)
- [x] Multi-container requirement documented
- [x] Apache 2.0 license confirmed
- [x] CNCF graduated status confirmed

## Files

| File | Status |
|------|--------|
| `harbor.snippet.yaml` | Created (partial) |
| `harbor.compatibility.yaml` | Created |
| `harbor.frontmatter.json` | Created |
| `harbor.research.md` | Created |

## References

1. [Harbor Documentation](https://goharbor.io/docs/)
2. [Harbor GitHub](https://github.com/goharbor/harbor)
3. [Harbor Installation](https://goharbor.io/docs/2.11.0/install-config/)
4. [CNCF Harbor](https://www.cncf.io/projects/harbor/)
5. [Harbor API](https://editor.swagger.io/?url=https://raw.githubusercontent.com/goharbor/harbor/main/api/v2.0/swagger.yaml)
