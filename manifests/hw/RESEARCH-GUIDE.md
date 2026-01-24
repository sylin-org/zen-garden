# Hardware Manifest Research Guide

**Purpose:** Methodology for researching and onboarding new hardware manifests into Zen Garden.

**Audience:** Agentic AI or human contributor creating hw manifests.

---

## Overview

A hardware manifest enables Zen Garden to:
1. **Identify** the hardware a stone is running on
2. **Manage firmware** updates for greenlit devices
3. **Guide service placement** based on hardware capabilities
4. **Advise operators** on hardware-service fit

Each hw manifest requires deep research to ensure accuracy. This guide provides the methodology.

---

## Research Phases

### Phase 1: Product Identification

**Goal:** Understand what the product actually is, including all variants.

#### 1.1 Official Product Information

Search for and document:
- [ ] Official product page (vendor website)
- [ ] Product datasheet / spec sheet (PDF)
- [ ] Product SKUs and model numbers
- [ ] End-of-life / support status

**Key questions:**
- Is this a single product or a product family?
- What variants exist under this product name?
- Is it still supported by the vendor?

#### 1.2 Variant Analysis

**Critical:** Many products ship with different specs under the same name.

Research:
- [ ] CPU variants (different processors offered)
- [ ] RAM configurations (soldered vs upgradeable, capacities)
- [ ] Storage options (eMMC, SSD, HDD sizes)
- [ ] Regional variants (different specs by market)

**Document each variant separately.** A single product name may need multiple manifests or a single manifest with variant detection rules.

**Decision criteria:**
- Same dmidecode `system-product-name` but different CPUs → likely needs variant handling
- Different `system-product-name` → separate manifests
- Same specs, different RAM/storage → single manifest (RAM/storage detected separately)

#### 1.3 Source Verification

For each claim, require **two independent sources**:
1. Official vendor documentation
2. Community verification (forum posts, reviews, teardowns)

**Red flags:**
- Specs only found on reseller sites (often wrong)
- Conflicting information between sources
- No community validation of official specs

---

### Phase 2: Technical Specifications

**Goal:** Document hardware characteristics that affect Zen Garden operation.

#### 2.1 CPU Analysis

| Property | How to Find | Why It Matters |
|----------|-------------|----------------|
| Model | Vendor specs, ARK | Service compatibility |
| Architecture | ARK, Wikipedia | Container image selection |
| Core/Thread count | ARK | Workload capacity |
| Base/Boost clock | ARK | Performance expectations |
| TDP | ARK | Power/thermal planning |
| Instruction sets | ARK, CPU-Z database | **Critical for AI/ML** |

**Instruction set checklist:**
- [ ] SSE4.1/4.2
- [ ] AVX (important for Ollama/LLMs)
- [ ] AVX2 (better performance)
- [ ] AVX-512 (rare on low-power)
- [ ] AES-NI (encryption acceleration)

**Sources:**
- Intel ARK: https://ark.intel.com/
- AMD Product Specs: https://www.amd.com/en/products/specifications/
- WikiChip: https://en.wikichip.org/

#### 2.2 Memory Analysis

| Property | How to Find | Why It Matters |
|----------|-------------|----------------|
| Type | Vendor specs | Compatibility |
| Soldered vs SODIMM | Teardowns, forums | Upgradeability |
| Max capacity | Vendor specs, community testing | Service limits |
| Channels | Vendor specs | Performance |

**Community testing often reveals higher max RAM than vendor specs.** Document both official and tested limits.

#### 2.3 Storage Analysis

| Property | How to Find | Why It Matters |
|----------|-------------|----------------|
| Type | Vendor specs | **Write endurance (eMMC concern)** |
| Interface | Vendor specs | Speed, compatibility |
| Capacity options | Vendor specs | Service data needs |
| Expansion | Teardowns | Upgrade path |

**eMMC warning:** Many thin clients use eMMC which has limited write cycles. This is critical for database services.

#### 2.4 Network Analysis

| Property | How to Find | Why It Matters |
|----------|-------------|----------------|
| Ethernet speed | Vendor specs | Throughput |
| WiFi | Vendor specs | Optional connectivity |
| Ports | Vendor specs | Multi-homing |

#### 2.5 Power & Thermal

| Property | How to Find | Why It Matters |
|----------|-------------|----------------|
| TDP | Vendor specs, reviews | Cooling needs |
| Idle power | Reviews, community testing | Operating cost |
| Load power | Reviews, community testing | Capacity planning |
| Cooling type | Vendor specs | Noise, reliability |

**Fanless devices** are excellent for always-on operation but may thermal throttle under sustained load.

---

### Phase 3: System Identification

**Goal:** Determine exact dmidecode strings for detection.

#### 3.1 dmidecode Strings

The manifest needs exact strings that `dmidecode` reports. These are used for hardware detection.

**Required fields:**
```bash
dmidecode -s system-manufacturer    # e.g., "Dell Inc."
dmidecode -s system-product-name    # e.g., "Wyse 5070"
dmidecode -s system-version         # e.g., "Extended" (may distinguish variants)
dmidecode -s bios-version           # e.g., "1.10.0"
```

**Optional fields (for variant detection):**
```bash
dmidecode -s baseboard-product-name
dmidecode -s processor-version
```

#### 3.2 Finding dmidecode Strings

**Sources (in order of reliability):**

1. **Actual hardware** - Best source, run dmidecode yourself
2. **Linux Hardware Database** - https://linux-hardware.org/
   - Search by product name, find probe reports
   - Contains actual dmidecode output from real systems
3. **Community forums** - ServeTheHome, Reddit r/homelab
   - Search for "[product name] dmidecode" or "[product name] linux"
4. **Bug reports** - Often contain system info
   - Search vendor bug trackers, kernel bugzilla

**Validation requirement:** dmidecode strings must come from actual hardware reports, not guessed from product names.

#### 3.3 Variant Detection Strategy

If variants exist with the same `system-product-name`:

**Option A: Single manifest with conditions**
```yaml
identity:
  system_manufacturer: "Dell Inc."
  system_product_name: "Wyse 5070"
  # Variants detected by additional fields
  variants:
    - name: "extended"
      system_version: "Extended"
    - name: "thin"
      system_version: "Thin"
```

**Option B: Separate manifests**
- `wyse-5070-extended.manifest.yaml`
- `wyse-5070-thin.manifest.yaml`

**Choose based on:** How different are the variants? If CPU differs significantly, separate manifests. If just RAM/storage differs, single manifest.

---

### Phase 4: Firmware Management

**Goal:** Document how to update firmware on this hardware.

#### 4.1 fwupd/LVFS Support

**Check LVFS device list:** https://fwupd.org/lvfs/devices/

Search for:
- Vendor name
- Product name
- GUID (from dmidecode if available)

**Document:**
- [ ] Is device in LVFS?
- [ ] What firmware components are updatable? (BIOS, EC, etc.)
- [ ] Last update date
- [ ] Current version in LVFS

#### 4.2 Vendor Update Tools

If not in LVFS, research vendor-specific tools:

| Vendor | Tool | Notes |
|--------|------|-------|
| Dell | Dell Command Update, fwupd | Many Dell devices in LVFS |
| HP | HP BIOS Update | Usually Windows-only |
| Lenovo | Lenovo System Update | Some in LVFS |

**Document:**
- [ ] Vendor update tool name
- [ ] Linux support?
- [ ] Download URL for manual updates
- [ ] Update procedure

#### 4.3 BIOS Version History

Research:
- [ ] Current latest version
- [ ] Release notes / changelog
- [ ] Known issues fixed
- [ ] Known issues introduced

**Sources:**
- Vendor support/download pages
- Community forums (issues reported)

#### 4.4 Update Requirements

Document:
- [ ] Requires reboot? (almost always yes for BIOS)
- [ ] Requires AC power?
- [ ] Minimum battery level?
- [ ] Any prerequisites?

---

### Phase 5: Service Compatibility Analysis

**Goal:** Determine which Zen Garden services run well on this hardware.

#### 5.1 Capability Mapping

Map hardware specs to service requirements:

| Hardware Trait | Affects Services | How |
|----------------|------------------|-----|
| No AVX | ollama, ML workloads | Very slow or non-functional |
| eMMC storage | mongodb, postgresql | Write wear concern |
| Low RAM (<4GB) | Most databases | May not fit |
| Low RAM (<8GB) | ollama, elasticsearch | Limited or non-functional |
| Fanless | All | May throttle under load |

#### 5.2 Cross-Reference with SW Manifests

For each sw manifest in `manifests/sw/`:
1. Read its `.compatibility.yaml`
2. Check if any rules would trigger for this hardware
3. Document in hw `.compatibility.yaml`

**Example:** `ollama.compatibility.yaml` has `celeron-avx-warning` rule. If hw lacks AVX, add ollama to `not_recommended`.

#### 5.3 Community Experience

Search for real-world usage reports:
- "[product name] homelab"
- "[product name] docker"
- "[product name] [service name]"

**Sources:**
- Reddit r/homelab, r/selfhosted
- ServeTheHome forums
- YouTube reviews/tutorials

Document actual user experiences, not just theoretical compatibility.

---

### Phase 6: Sourcing Information

**Goal:** Help operators acquire this hardware.

#### 6.1 Availability

- [ ] Still manufactured?
- [ ] Available refurbished?
- [ ] Common on secondary market?

#### 6.2 Pricing

Research current market prices:

| Condition | Source | Typical Price |
|-----------|--------|---------------|
| New | Official, Amazon | $XXX |
| Refurbished | Amazon Renewed, vendor | $XXX |
| Used | eBay, Craigslist | $XXX |

**Note:** Prices vary significantly by configuration. Document price ranges for common configs.

#### 6.3 What to Look For

Advise operators on:
- Recommended minimum specs (RAM, storage)
- Common issues to check
- Accessories included/needed
- Warranty considerations

---

## Output Format

### Required Files

For each hardware manifest, create:

```
manifests/hw/{vendor}/{model}.manifest.yaml
manifests/hw/{vendor}/{model}.compatibility.yaml
manifests/hw/{vendor}/{model}.frontmatter.json
manifests/hw/{vendor}/{model}.research.md
```

### Validation Checklist

Before submitting:

- [ ] dmidecode strings verified from actual hardware reports
- [ ] All specs have two independent sources
- [ ] Firmware update method tested or documented from reliable source
- [ ] Service compatibility cross-referenced with sw manifests
- [ ] Variant handling documented
- [ ] Research.md includes all sources with links

---

## Research Sources Reference

### Official Sources
- Intel ARK: https://ark.intel.com/
- AMD Specs: https://www.amd.com/en/products/specifications/
- Dell Support: https://www.dell.com/support/
- HP Support: https://support.hp.com/
- Lenovo Support: https://support.lenovo.com/

### Community Sources
- Linux Hardware Database: https://linux-hardware.org/
- ServeTheHome Forums: https://forums.servethehome.com/
- Reddit r/homelab: https://reddit.com/r/homelab
- Reddit r/selfhosted: https://reddit.com/r/selfhosted

### Firmware Sources
- LVFS Device List: https://fwupd.org/lvfs/devices/
- fwupd GitHub: https://github.com/fwupd/fwupd

### Pricing Sources
- eBay (completed listings for real prices)
- Amazon Renewed
- Newegg Refurbished

---

## Example Research Session

**Product:** Dell Wyse 5070

**Phase 1:** Find official product page, identify Extended vs Thin variants, note different CPU options (J4105, J5005).

**Phase 2:** Document J4105 specs from Intel ARK, note no AVX. Document eMMC storage concern.

**Phase 3:** Search linux-hardware.org for "Wyse 5070", find dmidecode reports, note exact strings.

**Phase 4:** Search LVFS for "Dell Wyse", check if 5070 is listed. Research Dell Command Update for Linux.

**Phase 5:** Cross-reference with ollama.compatibility.yaml (celeron-avx-warning applies). Check mongodb (eMMC concern).

**Phase 6:** Search eBay completed listings for "Wyse 5070 Extended 8GB", document typical prices.

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-24 | Initial guide |
