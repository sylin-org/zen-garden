---
audience: [contributor, maintainer, ai]
doc_type: adr
status: proposed
last_verified: 2026-05-27
canonical: true
---

# ORCH-0040: Snapshot Image Capture by Registry Reference

**Status**: Proposed
**Date**: 2026-05-27
**Deciders**: leo (architect)
**Tags**: offerings, snapshots, replication, storage, docker
**Refines**: [ORCH-0039](ORCH-0039-seed-based-offering-replication.md) — replaces its M2 "always `docker save`" image transport with a reference-first model, using the `ImageTransport` discriminator ORCH-0039 reserved for exactly this.

---

## Context

ORCH-0039 §"Seed metadata schema" captures an offering snapshot as: commit the running container to a `zen-harvest/<encoded_fqn>:<timestamp>` image, `docker save` that image into `<store>/<id>/image.tar`, then archive the volumes. The image tarball is the largest artifact in a snapshot — ~880 MB for mongodb, versus a ~238 MB volume archive.

Two facts, surfaced while fixing a snapshot-scheduler runaway that filled a stone's disk, make the per-snapshot image tarball wasteful:

1. **The restore path never uses it (for registry-backed offerings).** `plant` (`src/moss/src/infra/plant.rs`) calls `load_image(image.tar)` and then recreates the container from `compiled_to_container_spec(&compiled)`, whose `image` field is the **offering manifest's** image (`compiled.image`, e.g. `mongo:7.0`). `install_service` *pulls that* and creates the container from it. The loaded `zen-harvest` image is never referenced again. The meaningful restored state is the **volumes**; the image is reconstructed from the offering manifest, not from the snapshot.

2. **Consecutive snapshots duplicate near-identical bytes.** Every periodic capture saves a full image export, even though the base image is unchanged between snapshots. With keep-5 retention (ORCH-0040 era) across multiple offerings, the store holds many copies of essentially the same image. On `stone-golden-summit` this manifested as 850+ leaked `zen-harvest/*` images in the Docker store and gigabytes of redundant tarballs.

For the offerings the garden actually runs (mongo, qdrant, searxng, weaviate, postgres, …) the container image is a registry image: it is reproducible by digest. Saving its bytes per snapshot stores something already addressable by a 32-byte digest.

The exception is images that are **not** registry-reproducible — locally built images, `docker commit` results, and image-direct offerings loaded from a tarball. For these there is no registry to pull from, so the bytes must travel with the snapshot for a cross-stone plant to work (ORCH-0039 M3).

---

## Decision

Snapshot image capture becomes **reference-first**:

1. **Registry transport (default).** When the offering's running image has a registry digest (`RepoDigests` is non-empty), the snapshot records that digest-pinned reference (`repo@sha256:…`) and stores **no image bytes**. No `docker commit`, no `docker save`, no `image.tar`. At plant time the image is reproduced by reference — which is what `plant` already does via the offering manifest; the recorded digest additionally documents the exact image the snapshot was taken against.

2. **DockerSave transport (fallback).** When the running image has no registry digest (locally built / committed / image-direct), the snapshot falls back to the ORCH-0039 behaviour: commit the container, `docker save` to `image.tar`, dispose of the committed Docker image after save. This keeps such snapshots self-contained for cross-stone plant.

The `ImageTransport` enum, reserved by ORCH-0039 as a forward-compatible discriminator, gains the `Registry` variant:

```rust
pub enum ImageTransport {
    DockerSave, // bytes in image.tar — self-contained, used for non-registry images
    Registry,   // reproduced by digest at plant time — no local bytes
}
```

For `Registry` snapshots, `SnapshotImage` carries `ref_string = "<repo>@sha256:<digest>"`, `size_bytes = 0`, `sha512 = ""` (there is no local artifact to hash). For `DockerSave`, the fields are unchanged.

`plant` branches on the transport: it loads `image.tar` only for `DockerSave` snapshots and skips the load for `Registry` snapshots (the container is recreated from the offering manifest's image regardless).

Transport selection is per-capture and automatic — no manifest field, no user choice. An offering whose image is registry-backed today and locally-built tomorrow gets the correct transport each time.

---

## Consequences

**Positive**

- A registry-backed offering's snapshot drops from ~880 MB + volume to just the volume archive. Periodic snapshots stop committing and saving images entirely for the common case — no commit pause, no multi-hundred-MB write, no Docker image to dispose of.
- The Docker image store no longer accumulates `zen-harvest/*` tags from periodic captures (the DockerSave fallback still disposes of the committed image after save, per the runaway fix).
- Snapshot capture is dramatically faster and lighter for the common case (a digest inspect versus a commit + full save).

**Negative / trade-offs**

- A `Registry` snapshot is **not self-contained**: planting it requires the image to be pullable (or already present locally). On the same stone the image is present (the source container runs on it); cross-stone (M3) plant of a `Registry` snapshot requires the target stone to reach a registry that serves the digest. Non-registry images keep the self-contained `DockerSave` path, so the only exposure is a registry image that has since become unpullable. This is acceptable for a self-hosted garden where images are public-registry-backed and typically already cached on peers.
- Older Moss builds cannot deserialize a `Registry` manifest (unknown enum variant). Cross-stone replication therefore requires all participating stones to run this version or later. Single-stone capture/plant is unaffected; old `DockerSave` manifests remain readable by new builds.
- The recorded digest pins the image at capture time, but `plant` recreates from the *current* offering manifest's image (existing behaviour, with `digest_drift` detection unchanged). The snapshot's digest is provenance, not an override.

**Migration**

- No migration of existing snapshots is required. `DockerSave` manifests already on disk plant exactly as before. New captures choose their transport automatically.

---

## Alternatives considered

- **Content-addressed image store (dedup `image.tar` by digest).** Key saved tarballs by image digest under a shared `snapshots/_images/<digest>.tar` and have manifests reference them; the reconcile sweep reaps unreferenced blobs. This dedups the `DockerSave` path but still stores bytes the registry already has. Kept as a possible future optimisation for the fallback path only; not worth the complexity when the common case stores no bytes at all.
- **Local registry / OCI image layout for layer-level dedup.** Push committed images to the local registry (stones already run one) or copy to an OCI layout, deduping layers across snapshots and offerings; plant pulls. This is the most scalable transport and remains the natural choice if M3 cross-stone replication needs to ship non-registry images efficiently. Deferred: it adds a registry dependency to the capture path for a benefit the reference-first model already delivers for registry images.
- **Keep ORCH-0039 as-is.** Rejected: it stores hundreds of megabytes per snapshot that the restore path does not use for the offerings the garden runs.

---

## References

- [ORCH-0039](ORCH-0039-seed-based-offering-replication.md) — seed-based replication; reserves the `ImageTransport` discriminator.
- `src/moss/src/infra/snapshot.rs` — capture flow (`capture_into`).
- `src/moss/src/infra/plant.rs` — restore flow; `compiled_to_container_spec` shows the container is recreated from the offering manifest's image.
- `src/moss/src/domain/snapshot.rs` — `SnapshotManifest`, `SnapshotImage`, `ImageTransport`.
