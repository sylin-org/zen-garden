---
audience: [maintainer, contributor, ai]
doc_type: assessment
status: current
last_verified: 2026-07-13
---

# Zen Garden Application Lifecycle Assessment, July 2026

This is a point-in-time, evidence-backed assessment of Zen Garden's application lifecycle.
It distinguishes current behavior from partially connected machinery and intended behavior.
It is product memory, not a specification and not a claim that every inspected path has been
exercised end to end.

## Scope and method

The assessment traced the current repository from Stone creation through offering installation,
discovery, operation, health monitoring, reconciliation, backup, update, migration, and retirement.
It used the repository, code, embedded manifests, tests, current documentation, and surface ledger
as authority.

The principal evidence was:

- the USB creator, first-boot coordinator, and pre-install handler;
- the Rake command manifest and offering, discovery, lifecycle, backup, and nourishment commands;
- the Moss offering lifecycle, Docker runtime, health monitor, reconciliation coordinator, jobs,
  snapshots, harvests, plant flow, and orchestration loop;
- all 51 checked-in software offering snippets and their frontmatter;
- the ignored Docker snapshot integration tests and the repository surface ledger;
- the README, offering lifecycle guide, and update journey documents.

No source was changed and no test suite was run during the investigation. The Zen Garden worktree
already contained unrelated work from another agent; the inspected lifecycle sources were left
untouched.

## Executive conclusion

Zen Garden already has a compelling application lifecycle, but it is strongest from intent through
operation and same-Stone container recovery. Safe updates, durable retirement, and cross-Stone state
recovery are promising but do not yet form one trustworthy end-to-end contract.

Zen Garden's lifecycle unit is a named **offering**, such as `mongodb` or `mongodb::dev`, not merely
a Docker container:

```text
Stone bootstrap
    |
service intent --> manifest + hardware negotiation
    |
container + durable data + resolved ports
    |
garden discovery + connection URI
    |
health monitoring + reconciliation + inspection
    |
backup / update / migration / retirement
```

The most important product invariant is:

> The container is disposable. The offering's identity, configuration, data location, port
> allocation, and discoverability are the durable intent.

That is a stronger and more distinctive product story than "orchestration for old computers."

## Lifecycle capability map

| Lifecycle moment | Current behavior | Assessment |
|---|---|---|
| Turn hardware into a Stone | The USB builder creates an unattended Debian installer. First boot assigns identity and can process a pre-install offering manifest. | Real, but currently a build-your-own delivery workflow rather than a polished image release. |
| Request a service | `garden-rake offer mongodb` resolves a checked-in manifest, hardware compatibility, and possible fallback image. | Strong. |
| Place it | Explicit Stone selection, placement recommendations, hardware preferences, and compatibility failures exist. | Strong locally; broader placement needs real-garden exercise. |
| Install it | Moss pulls the image, writes seeded config, creates isolated persistent bind directories, resolves ports, starts the container, and records the offering. | Strong. |
| Discover and use it | `garden-rake find mongodb` returns matching services, Stones, and connection URIs. JSON and URI-only forms support scripts and agents. | Strong. |
| Extend it | Capability manifests can add models or other sub-capabilities. `find ... ensure` can combine base provisioning and capability installation. | Valuable and unusually agent-friendly. |
| Operate it | Rest, wake, restart, logs, events, configuration patches, resource observation, and capability refresh are present. | Good for managed offerings. |
| Recover a missing runtime | The health monitor detects a missing managed container and recreates it from intent with bounded concurrency and exponential backoff. | One of the strongest lifecycle capabilities. |
| Back it up | Manual A/B nurturing backups, seed-bank replication, and a newer snapshot engine exist. | Functional pieces, but two overlapping models and important consistency gaps. |
| Update it safely | Basic container recreation works. A safer collect/nourish/water ceremony exists in code. | Basic update is real; automatic guarded rollback is not the normal command path. |
| Move it between Stones | Snapshot capture, artifact transfer, and plant APIs exist. | Experimental and incompletely integrated. |
| Remove it | `remove` and `uproot` exist. | Their documented distinction is not implemented reliably. |

## What works especially well

### Intent to useful infrastructure is genuinely short

The manifest-backed installation path performs meaningful operational work:

- hardware compatibility evaluation;
- curated image selection and fallbacks;
- persistent per-FQN data directories;
- generated configuration files;
- GPU and resource settings;
- restart policy;
- healthcheck configuration;
- stable port negotiation;
- topology publication;
- scheduled manifest tasks.

The central path begins in
[`service_lifecycle.rs`](../../src/moss/src/domain/service_lifecycle.rs), with deployment execution in
[`job_executors.rs`](../../src/moss/src/tasks/job_executors.rs).

The current catalog contains 51 offering snippets across 18 category directories. The README's
older count of 31 is stale.

### Port negotiation is a substantive lifecycle feature

Port allocations are serialized, checked against listening sockets and stopped Docker containers,
remapped when needed, and persisted so a recreated service tries to retain its published port.
See [`port.rs`](../../src/moss/src/docker/port.rs) and
[`port_ledger.rs`](../../src/moss/src/docker/port_ledger.rs).

A supportable public claim is:

> Zen Garden negotiates a usable local port and remembers the result across recreation.

One boundary needs attention: the port-53 policy can disable `systemd-resolved` and replace
`/etc/resolv.conf` automatically. That is consequential host mutation without a visible plan or
explicit approval. Routine conflict handling should prefer safe remapping. Invasive remediation
needs preview and consent.

### Runtime reconciliation embodies the product philosophy

Every 30 seconds, Moss reconciles container status, health, resources, ports, protocol information,
and topology mounts. Missing managed containers are rebuilt with a 30, 60, 120, 240, and 480 second
retry schedule and marked degraded after five failures.

See [`health_monitor.rs`](../../src/moss/src/tasks/health_monitor.rs) and
[`offering_reconciliation.rs`](../../src/moss/src/tasks/offering_reconciliation.rs).

This supports a strong, narrow continuity promise:

> If a managed container disappears on a surviving Stone, the garden can reconstruct its runtime
> from recorded offering intent while preserving Zen-managed bind data.

That is currently more defensible than claiming automatic recovery from the loss of an entire
machine.

### The three offering modes are the right model

Zen distinguishes:

- **Managed**: Moss owns a container lifecycle.
- **Adopted**: an existing native or containerized service is detected and monitored.
- **Borrowed**: an external service is registered for discovery.

The model is recorded in
[`OFFER-0005`](../decisions/OFFER-0005-offering-modes.md).

Adopted start, stop, and restart commands are stored in state but are not wired into the ordinary
lifecycle service, whose operations require managed offerings. Publicly, adopted should currently
mean "observed and discoverable," not broadly "managed with optional restart."

## Important truth gaps

### Readiness is optimistic

Installation marks an offering `Running` and `Healthy` immediately after Docker creation. The later
health monitor can correct that state, but job completion is not synchronous application readiness.

`find ... ensure` waits for the Tools projection, but that projection treats `Running` as ready rather
than proving application health. Therefore:

- `offer` proves that deployment completed;
- it does not yet prove that MongoDB accepted a real connection;
- `find ensure` may report ready during the optimistic window.

The eventual delightful single command is:

```text
garden-rake find mongodb ensure --format uri
```

Until readiness is grounded in real health, the responsible public demonstration is:

```text
garden-rake offer mongodb
garden-rake find mongodb --format uri
```

### Safe updates are not the ordinary update behavior

The normal single-service upgrade path pulls the selected image, removes the old container, installs
the replacement, and marks the offering running. If recreation fails, it restores the registry
status, not the previous container or image.

The stronger collect -> nourish -> water implementation, including health waiting and rollback
intent, exists in [`domain/ceremony/nourish.rs`](../../src/moss/src/domain/ceremony/nourish.rs), but
the regular `upgrade` and garden-wide `nourish` paths do not call it.

Consequently, the README's "multi-phase safe updates" and journey documents' automatic rollback
stories describe intended behavior, not the normal CLI's current guarantee.

### `remove` and `uproot` have nearly identical effects

Both commands enter the same implementation. The practical difference is the emitted event. Both
remove the container and registry entry. Neither explicitly deletes the normal host bind directories
where offering data lives.

This creates three conflicting stories:

- the CLI says `remove` only releases registry ownership;
- examples say it removes the container but preserves volumes;
- `uproot` says it irreversibly destroys all data.

For normal curated offerings, the third statement is not supported.

### Recovery has two competing generations

The older nurturing system provides:

- A/B local slots;
- manual trigger and restore;
- optional seed-bank replication;
- a CLI surface through `garden-rake backup`.

Its harvest restore verifies volume checksums, but capture does not quiesce or pause the application
first, and normal restore is not an all-or-nothing staged replacement.

The newer snapshot system is architecturally better:

- image reference or image archive;
- managed and external volume archives;
- checksums and manifests;
- asynchronous jobs;
- local and cross-Stone plant;
- staged volume swaps;
- five-snapshot retention;
- automatic capture roughly every four hours.

See [`snapshot.rs`](../../src/moss/src/infra/snapshot.rs),
[`snapshot_scheduler.rs`](../../src/moss/src/infra/snapshot_scheduler.rs), and
[`plant.rs`](../../src/moss/src/infra/plant.rs).

The newer route still has significant gaps:

- none of the 51 checked-in offerings defines an explicit ceremony policy; all inherit the default
  `unsafe` behavior;
- periodic capture pauses containers but proceeds without a pause if pausing fails;
- captured artifact hashes are recorded but not verified by the plant path;
- the captured image reference is not used when rebuilding the container; the current compiled
  manifest image is used instead;
- a first-time plant does not directly register the offering and may rely on later orphan adoption;
- failure after replacing volumes is not transactionally rolled back;
- failure to become healthy within 120 seconds is only a warning, and the job still succeeds;
- cross-Stone fetching currently uses plain HTTP and trust in the LAN.

This is valuable work in progress, but it is not yet "restore this exact application safely on
another Stone."

### Generic Primary/Replica state is ahead of generic data replication

Many stateful offering manifests opt into elected coordination. A second instance may be assigned
`Joining`, but the generic orchestration loop explicitly leaves `Joining` as a no-op pending a later
synchronization phase.

MongoDB has a dedicated orchestrator capable of service-specific work, but Zen cannot generalize
that behavior to every offering marked elected. Therefore the broad statement that applications
automatically reconnect to another Stone after one dies is unsupported.

Current support is narrower:

- rediscovery when another usable instance already exists;
- container-level reconstruction on the same surviving Stone;
- specialized coordination where a functioning dedicated orchestrator is present.

It is not universal stateful failover.

## Responsible public narrative

A supportable application-lifecycle story is:

> Turn a spare machine into a Stone, then ask it to offer MongoDB. Zen Garden selects a curated
> image for that hardware, creates durable storage, negotiates a usable port, starts the service,
> and publishes its connection URI. Moss keeps watching: if the container disappears, it
> reconstructs the runtime from the same offering intent. You operate the service by name rather
> than by remembering which Docker command created it.

The candid boundary is:

> Cross-Stone replication, guarded application updates, and exact snapshot-based migration are
> active work. Treat the current release as a home-lab substrate under active development, not a
> production high-availability platform.

## Website implications

An application-lifecycle section should show five moments:

1. Prepare a Stone from USB.
2. Offer MongoDB.
3. Find its actual connection URI.
4. Show what Moss continues tending: health, ports, data, logs, and runtime reconstruction.
5. Name the current boundary: Stone loss, replicated state, and automatic update rollback remain
   service-specific or unfinished.

The current website entry should eventually:

- replace "when one dies, they reconnect to another" with same-Stone reconstruction and candid
  service-specific failover language;
- replace "~31 offerings" with 51 checked-in templates, while making clear that catalog presence
  is not equivalent to production certification;
- replace "safe updates ... with automatic rollback" with a work-in-progress statement;
- change the first useful scenario from discovery alone to `offer` followed by `find`;
- preserve e-waste, sovereignty, and the holistic garden narrative.

## Recommended implementation blocks

### 1. Make readiness truthful

Drive `Running`, Tools readiness, and `find ensure` from real container or application health. Have
`offer` finish by printing the resolved URI or a precise "deployed, still becoming ready" state.

### 2. Fix lifecycle semantics

Define and test exact contracts for rest, wake, restart, release, remove, and uproot. Make destructive
data deletion explicit, previewable, and restricted to Zen-managed paths.

### 3. Make the safe ceremony the update path

Route `upgrade` and application portions of `nourish` through one collect, update, verify, and rollback
transaction. Restore the exact previous image and data, and fail the command when post-update health
fails.

### 4. Converge the backup systems

Select one public snapshot model. Add explicit ceremony policies to every offering, verify artifacts
before restore, use captured image identities, register planted offerings synchronously, and make an
unhealthy plant fail.

### 5. Bound orchestration claims per offering

Only advertise failover for services with a functioning replication orchestrator and an exercised
recovery test. An elected manifest without synchronization must not imply a usable replica.

### 6. Complete the USB product

Publish a signed, checksummed image or installer bundle. The current USB creator requires locally
built packages despite documentation that implies downloadable releases, and the repository has no
local release tags.

### 7. Add lifecycle tripwires

The snapshot Docker tests exist but are ignored by default, and the surface ledger records no CI.
Add exercised tests for:

- install -> real readiness -> find;
- missing-container reconstruction;
- safe update rollback;
- remove and uproot data behavior;
- cross-Stone snapshot integrity.

## Product judgment

The everyday lifecycle already expresses the holistic Zen Garden idea well. The next milestone is
not adding more lifecycle verbs. It is making every existing verb tell the truth about when an
application is ready, what survives, what can be recovered, and what destructive action will occur.

