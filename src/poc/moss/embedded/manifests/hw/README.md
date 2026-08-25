# Zen Garden Hardware Manifests

Hardware definitions for "greenlit" devices that Zen Garden can identify and manage.

## Purpose

Hardware manifests enable:
1. **Detection** - Identify the hardware a stone is running on
2. **Firmware Management** - Update BIOS/firmware on supported devices
3. **Service Guidance** - Recommend services based on hardware capabilities
4. **Operator Advice** - Provide hardware-specific configuration tips

## Structure

```
manifests/hw/
├── README.md                    # This file
├── RESEARCH-GUIDE.md           # How to research new hardware
├── dell/
│   ├── wyse-5070.manifest.yaml      # Identity + firmware config
│   ├── wyse-5070.compatibility.yaml # Service fit rules
│   ├── wyse-5070.frontmatter.json   # Catalog metadata
│   └── wyse-5070.research.md        # Research documentation
├── hp/
│   └── t520.* (future)
└── lenovo/
    └── thinkcentre-m710q.* (future)
```

## File Purposes

| File | Purpose |
|------|---------|
| `.manifest.yaml` | How to detect + update this hardware |
| `.compatibility.yaml` | Which services run well/poorly |
| `.frontmatter.json` | Catalog entry for discovery/search |
| `.research.md` | Research notes, validation, sources |

## Adding New Hardware

See [RESEARCH-GUIDE.md](./RESEARCH-GUIDE.md) for the complete methodology.

### Quick Checklist

1. **Research the hardware thoroughly**
   - Official specs from vendor
   - Community reports (linux-hardware.org)
   - Real-world usage (Reddit, forums)

2. **Verify dmidecode strings**
   - Must come from actual hardware probes
   - Don't guess from product names

3. **Check firmware update support**
   - fwupd/LVFS preferred
   - Document alternatives if not in LVFS

4. **Cross-reference with sw manifests**
   - Check compatibility rules in `manifests/sw/`
   - Document which services fit well/poorly

5. **Create all four files**
   - manifest, compatibility, frontmatter, research

## Greenlit Hardware

| Vendor | Model | Status | Key Traits |
|--------|-------|--------|------------|
| Dell | Wyse 5070 | ✅ Complete | Fanless, no AVX, eMMC |

## Detection Flow

1. Stone boots, Moss starts
2. Moss runs `dmidecode` to get system identity
3. Moss scans `manifests/hw/*/` for matching identity
4. If match found:
   - Stone is "greenlit" for that hardware
   - Capabilities API exposes hardware profile
   - Service recommendations are available
   - Firmware updates can be offered via `nourish stone`

## API Endpoints

```
GET  /api/v1/manifests/hw              # List all hw manifests
GET  /api/v1/manifests/hw/dell/wyse-5070  # Specific manifest
GET  /api/v1/stone/identity            # This stone's detected hardware
POST /api/v1/admin/stone/nourish       # Apply firmware update (greenlit only)
```

## Contributing

Hardware manifests require real-world validation. Contributions welcome for:
- New hardware models
- Corrections to existing manifests
- Service compatibility reports
- Firmware update experiences

See [RESEARCH-GUIDE.md](./RESEARCH-GUIDE.md) for contribution requirements.
