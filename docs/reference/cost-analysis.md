# Cost Analysis: Zen Garden vs Cloud Computing

**Version**: 1.0 | **Updated**: 2026-01-26 | **Status**: Reference

> A realistic cost comparison for small businesses and developers evaluating on-premise infrastructure managed by Zen Garden versus cloud alternatives.

---

## Executive Summary

Running three Dell Wyse 5070 thin clients with Zen Garden costs approximately **$26-50/year** in electricity while providing compute equivalent to **$840-1,440/year** in cloud services (70-90% savings). With a one-time hardware cost of ~$150-300 (used equipment), the investment pays for itself in 2-4 months.

---

## Test Hardware Profile

### Dell Wyse 5070 Thin Client (x3)

| Specification        | Value                                       |
| -------------------- | ------------------------------------------- |
| **CPU**              | Intel Celeron J4105 (4 cores @ 1.5-2.5 GHz) |
| **RAM**              | 8 GB DDR4                                   |
| **Storage**          | 128-256 GB NVMe (typical upgrade)           |
| **Power (idle)**     | 6W per unit                                 |
| **Power (load)**     | ~17W per unit                               |
| **Form Factor**      | 1.3L ultra-compact                          |
| **Acquisition Cost** | $50-100/unit (eBay, 2026 prices)            |

### Measured Power Consumption (3-unit cluster)

| State                  | Power Draw       |
| ---------------------- | ---------------- |
| All idle               | 18W combined     |
| All under load         | 50W combined     |
| Typical mixed workload | ~25-35W combined |

---

## Cloud Cost Comparison

### Equivalent Cloud Instances

The Dell Wyse 5070 with Celeron J4105 provides approximately:

- **4 vCPUs** (burst capable)
- **8 GB RAM**
- **128-256 GB storage**
- **24/7 availability** (no spin-up time)

The closest cloud equivalents for a **single Wyse 5070**:

| Provider  | Instance Type | vCPU | RAM   | Monthly Cost | Annual Cost |
| --------- | ------------- | ---- | ----- | ------------ | ----------- |
| **AWS**   | t3.medium     | 2    | 4 GB  | ~$30         | $360        |
| **AWS**   | t3.large      | 2    | 8 GB  | ~$60         | $720        |
| **AWS**   | t3.xlarge     | 4    | 16 GB | ~$120        | $1,440      |
| **Azure** | B2s v2        | 2    | 8 GB  | ~$61         | $732        |
| **Azure** | B4s v2        | 4    | 16 GB | ~$121        | $1,452      |
| **Azure** | D2as v5       | 2    | 8 GB  | ~$63         | $756        |

**Note**: Cloud pricing for on-demand Linux instances, US East region, January 2026. Excludes storage, data transfer, and IP address costs.

### Why t3.large/B2s is the Fair Comparison

The Wyse 5070 provides:

- **4 physical cores** (vs 2 vCPUs with burstable performance)
- **8 GB dedicated RAM** (not shared)
- **Local NVMe storage** (faster than EBS gp3)
- **Zero cold start** (always running)
- **No network egress fees**
- **No data transfer charges**

A t3.large with similar always-on capability (no burst credits) would require t3.xlarge pricing for sustained workloads.

---

## Annual Cost Breakdown

### Zen Garden (3 Wyse 5070s)

| Category                  | Calculation                  | Annual Cost     |
| ------------------------- | ---------------------------- | --------------- |
| **Electricity (idle)**    | 18W × 24h × 365d × $0.12/kWh | **$18.92**      |
| **Electricity (mixed)**   | 30W × 24h × 365d × $0.12/kWh | **$31.54**      |
| **Electricity (load)**    | 50W × 24h × 365d × $0.12/kWh | **$52.56**      |
| **Hardware amortized**    | $225 ÷ 5 years               | **$45.00**      |
| **Total (low estimate)**  |                              | **$63.92/year** |
| **Total (high estimate)** |                              | **$97.56/year** |

_Electricity cost: US national average $0.12/kWh. Actual rates vary $0.08-0.30/kWh._

### Cloud Equivalent (3 instances, always-on)

| Provider  | Instance Type | Monthly | Annual     |
| --------- | ------------- | ------- | ---------- |
| **AWS**   | 3× t3.medium  | $90     | **$1,080** |
| **AWS**   | 3× t3.large   | $180    | **$2,160** |
| **Azure** | 3× B2s v2     | $183    | **$2,196** |
| **Azure** | 3× B4s v2     | $363    | **$4,356** |

**Add storage costs:**

- AWS EBS gp3 100GB × 3: ~$24/month → $288/year
- Azure Premium SSD 128GB × 3: ~$18/month → $216/year

**Add data transfer:**

- 100 GB egress/month: ~$9/month AWS → $108/year

**Realistic cloud total: $1,500-2,500/year**

---

## Savings Summary

| Metric         | Zen Garden            | Cloud (AWS t3.large) | Savings           |
| -------------- | --------------------- | -------------------- | ----------------- |
| **Year 1**     | $325 (incl. hardware) | $2,500               | **87%**           |
| **Year 2-5**   | $65-100/year          | $2,500/year          | **96%**           |
| **5-Year TCO** | $625                  | $12,500              | **$11,875 saved** |

---

## What You Get With Zen Garden

### Included (No Additional Cost)

| Feature                 | Cloud Equivalent     | Monthly Savings |
| ----------------------- | -------------------- | --------------- |
| Service discovery       | Route 53 / Azure DNS | $0.50-5/zone    |
| Container orchestration | ECS/AKS basic        | $0-72/month     |
| Firmware updates        | AWS Systems Manager  | $0 (included)   |
| Software updates        | Managed updates      | $0 (included)   |
| Basic monitoring        | CloudWatch basic     | $0 (included)   |
| Local storage           | EBS/Azure Disks      | $20-50/month    |
| Network (internal)      | VPC free tier        | $0              |

### Not Yet Included (Future)

| Feature             | Status   | Cloud Equivalent |
| ------------------- | -------- | ---------------- |
| Automated failover  | Planned  | Multi-AZ ($$$)   |
| Seed banks (backup) | Planned  | S3/Blob storage  |
| AWS Bridge          | Proposal | LocalStack ($$$) |

---

## AWS Bridge: Future Value Proposition

The proposed [AWS Bridge](../proposals/zen-garden-spec-aws-bridge.md) offering would provide AWS-compatible APIs backed by local services:

| AWS Service     | Bridge Backend      | Status   |
| --------------- | ------------------- | -------- |
| S3              | MinIO / Seed Banks  | Proposed |
| SQS             | Redis               | Proposed |
| DynamoDB        | MongoDB             | Proposed |
| Secrets Manager | Local encryption    | Proposed |
| Lambda          | Container execution | Proposed |

### Developer Value

- **Develop locally, deploy anywhere**: Same SDKs work on Zen Garden and AWS
- **No LocalStack license**: LocalStack Pro costs $35+/month/developer
- **Real backends**: Not mocks—actual databases and queues
- **Zero data egress**: Your data stays on your network

---

## Public Exposure: Serving Apps to the Internet

### Cloudflare Tunnel (Zero Trust)

With Cloudflare Tunnel, your Zen Garden can serve applications to the internet **without opening firewall ports** or needing a static IP:

| Cloudflare Feature  | Free Tier   | Paid (Pro)  |
| ------------------- | ----------- | ----------- |
| Tunnels (unlimited) | ✅ Free     | ✅ Included |
| Custom domains      | ✅ Free     | ✅ Included |
| SSL certificates    | ✅ Free     | ✅ Included |
| DDoS protection     | ✅ Basic    | Enhanced    |
| Bandwidth           | Unlimited\* | Unlimited   |
| Access policies     | 50 users    | Unlimited   |

\*Fair use policy applies; no hard limits for legitimate traffic.

**Setup cost**: $0 (free tier sufficient for small business)

**Cloud equivalent**:

- AWS ALB: $16+/month + $0.008/LCU-hour
- CloudFront: $0.085/GB (first 10TB)
- Elastic IP: $3.65/month
- **Total: $25-100+/month**

### UPS (Uninterruptible Power Supply)

For production workloads, a UPS provides:

- Clean power (protects hardware)
- Graceful shutdown on outage
- Ride-through for brief outages

| UPS Type             | Capacity | Runtime (50W load) | Cost |
| -------------------- | -------- | ------------------ | ---- |
| APC BE425M           | 425VA    | ~30 min            | $55  |
| CyberPower CP685AVRG | 685VA    | ~45 min            | $80  |
| APC BR1500MS2        | 1500VA   | ~2+ hours          | $200 |

**Recommendation for 3× Wyse 5070s (50W max)**:

- CyberPower CP685AVRG ($80) provides 45+ minutes
- Enough for graceful shutdown or brief outages

### Updated Total Cost of Ownership

| Component               | One-Time | Annual  | Notes             |
| ----------------------- | -------- | ------- | ----------------- |
| 3× Wyse 5070            | $225     | -       | eBay/surplus      |
| UPS (685VA)             | $80      | -       | 5-year lifespan   |
| UPS battery replacement | -        | $20     | Every 3-5 years   |
| Domain name             | -        | $12     | Optional          |
| Electricity             | -        | $65     | Mixed workload    |
| **Total Year 1**        | **$305** | **$77** | **$382**          |
| **Total Years 2-5**     | -        | **$85** | Including battery |

**Cloud equivalent for public-facing app**:

- 3× t3.large: $2,160/year
- ALB + CloudFront: $300-600/year
- Domain: $12/year
- **Total: $2,500-2,800/year**

**Savings with Cloudflare Tunnel**: Still **85-90%** vs cloud

---

## Storage Considerations

### Local vs Cloud Storage Costs

| Storage Type | Zen Garden       | AWS       | Azure       |
| ------------ | ---------------- | --------- | ----------- |
| 100 GB SSD   | One-time $10-20  | $8/month  | $9.60/month |
| 500 GB SSD   | One-time $40-60  | $40/month | $48/month   |
| 1 TB SSD     | One-time $60-100 | $80/month | $96/month   |

**Annual storage savings (1TB):**

- AWS: $80/month × 12 = $960
- One-time NVMe: $80
- **Savings: $880/year per TB**

### Seed Bank (Future)

When implemented, Seed Banks will provide:

- Local backup replication across stones
- Cultivation/harvesting for data portability
- No cloud storage egress fees

---

## Availability Considerations

### What Zen Garden Provides (Phase 1)

| Feature                   | Implementation                    |
| ------------------------- | --------------------------------- |
| Service health monitoring | 45-second offline detection       |
| Automatic discovery       | New stones join automatically     |
| Container self-heal       | Orphan adoption, restart policies |
| Firmware updates          | fwupd integration                 |
| Software updates          | Docker image pulls                |

### What's Coming (Phase 2+)

| Feature                | Status  | Impact                   |
| ---------------------- | ------- | ------------------------ |
| Intelligent placement  | Planned | Move services on failure |
| Rollback               | Planned | Undo bad updates         |
| Ceremonies             | Planned | Coordinated maintenance  |
| Multi-stone redundancy | Planned | Data replication         |

### Honest Limitations

- **No automatic failover** (yet): If a stone dies, services on it stop
- **No built-in HA**: Stateful workloads need external replication
- **Single-site**: Not designed for multi-datacenter (that's cloud's strength)

**Mitigation**: For critical workloads, use databases with built-in replication (MongoDB replica sets, PostgreSQL streaming replication) across multiple stones.

---

## When Cloud Still Makes Sense

Zen Garden is **not** trying to replace:

| Use Case                       | Recommendation             |
| ------------------------------ | -------------------------- |
| Global distribution            | Use cloud CDN/edge         |
| Massive scale (1000+ servers)  | Hyperscaler infrastructure |
| Strict SLA requirements        | Managed cloud services     |
| GPU compute (ML training)      | Cloud GPU instances        |
| Disaster recovery (geographic) | Cloud backup targets       |

**Hybrid approach**: Use Zen Garden for development, staging, and low-criticality production. Use cloud for global distribution and disaster recovery.

---

## Real-World Scenarios

### Scenario 1: Solo Developer

| Setup                                              | Annual Cost     |
| -------------------------------------------------- | --------------- |
| 1× Wyse 5070 (MongoDB, Redis, dev server)          | $22 electricity |
| Hardware (one-time)                                | $75             |
| **Year 1 total**                                   | **$97**         |
| **AWS equivalent** (t3.medium + RDS + ElastiCache) | **$3,600+**     |

### Scenario 2: Small Team (3 developers)

| Setup                                        | Annual Cost     |
| -------------------------------------------- | --------------- |
| 3× Wyse 5070 (shared dev/staging)            | $65 electricity |
| Hardware (one-time)                          | $225            |
| **Year 1 total**                             | **$290**        |
| **AWS equivalent** (3× t3.large + shared DB) | **$4,500+**     |

### Scenario 3: Small Business Production

| Setup                                          | Annual Cost      |
| ---------------------------------------------- | ---------------- |
| 5× Wyse 5070 (web, API, DB, cache, monitoring) | $100 electricity |
| Hardware (one-time)                            | $400             |
| **Year 1 total**                               | **$500**         |
| **AWS equivalent** (proper production setup)   | **$8,000+**      |

---

## Hardware Sourcing

### Recommended Sources (2026)

| Source                      | Typical Price | Notes          |
| --------------------------- | ------------- | -------------- |
| eBay                        | $50-100       | Most selection |
| Facebook Marketplace        | $40-80        | Local pickup   |
| IT Asset Disposition (ITAD) | $30-60        | Bulk pricing   |
| Corporate surplus auctions  | $20-50        | Inconsistent   |

### What to Look For

- **Memory**: 8GB minimum (upgradeable on most thin clients)
- **Storage**: M.2 NVMe slot (easy to upgrade)
- **CPU**: Intel Celeron J4xxx or Pentium Silver (good efficiency)
- **Network**: Gigabit Ethernet (essential)

### E-Waste Reclamation

Many thin clients are discarded when:

- Corporate refresh cycles (3-5 years)
- Office closures
- Thin client-to-VDI migrations

These devices have years of useful life remaining for containerized workloads.

---

## Methodology Notes

### Power Measurements

- Measured using Kill-A-Watt P3 meter
- 72-hour observation period
- Mixed workloads: MongoDB, Redis, nginx, dev containers
- Room temperature: 22°C

### Cloud Pricing

- AWS and Azure public pricing (January 2026)
- On-demand, no reserved instances
- US East regions
- Linux operating systems
- Excludes support plans

### Assumptions

- 5-year hardware lifespan (conservative)
- $0.12/kWh electricity (US average)
- No cooling costs included (thin clients don't need special cooling)
- No rack/hosting costs (devices sit on shelf/desk)

---

## Conclusion

For small businesses and developers running always-on development, staging, or small production workloads, Zen Garden on reclaimed hardware offers:

- **90%+ cost reduction** compared to cloud
- **Zero ongoing license fees**
- **Data sovereignty** (your data, your network)
- **Environmental benefit** (extending hardware life)
- **Learning opportunity** (real infrastructure, not abstractions)

The trade-off is manual failover and single-site limitations—acceptable for many workloads, especially when combined with proper database replication.

---

## Related Documentation

- [Staying Focused](../philosophy/staying-focused.md) - Project mission and user focus
- [AWS Bridge Proposal](../proposals/zen-garden-spec-aws-bridge.md) - Cloud API compatibility
- [Architecture Reference](../ARCHITECTURE-REFERENCE.md) - Technical overview
