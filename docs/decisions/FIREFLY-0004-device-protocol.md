---
audience: [developer, ai]
doc_type: decision
status: accepted
last_verified: 2026-04-14
canonical: true
---

# FIREFLY-0004: Firefly Device Protocol — Identity, Descriptor, and Provisioning Ritual

**Date**: 2026-04-14
**Status**: Accepted — **implementation pending**
**Depends on**: [FIREFLY-0001](FIREFLY-0001-v0-implementation.md), [FIREFLY-0002](FIREFLY-0002-esp8266-oled-device.md), [FIREFLY-0003](FIREFLY-0003-tdisplay-diorama.md)
**Pairs with**: [COMPANION-0012](COMPANION-0012-device-bus.md) (consumes this protocol)

## Context

Today every firefly variant emits a free-form CSV in response to the `I` command:

```
OK,firefly-oled,esp8266,128x64,dual-zone:yellow:16:blue:48,v0.2.0
```

The host parses by string split. New fields require string-position migration. There is no version field in the structural sense — the version is a `v0.2.0` string in slot 5 with no schema-evolution rules. There is no stable per-device identity — adapter state (brightness, label) is keyed by serial port path, which renumbers on reboot. There is no way for a host to distinguish a Zen Garden device from a random USB CDC device that happens to respond to `I` with a non-error string.

COMPANION-0012 introduces a device bus that needs structured input. Rather than retrofit the bus to a stringly-typed protocol, formalize the protocol as JSON, embed device identity into the protocol, and let the bus reject foreign devices structurally instead of guessing.

The local environment is the only consumer. No external users, no fielded devices outside our control, no migration burden. We can be opinionated.

## Decision

### 1. Device descriptor — the JSON schema

Every firefly device emits a single JSON object as its identification frame:

```json
{
  "device_id": "01938abc-de01-7234-89ab-cdef01234567",
  "family": "firefly",
  "variant": "oled",
  "version": "0.2.0",
  "processor": "esp8266",
  "hardware_id": "esp8266-a1b2c3",
  "display": {"resolution": "128x64", "type": "oled-dual-zone"},
  "capabilities": ["dashboard", "wipe-animations", "brightness"]
}
```

| Field | Required | Type | Notes |
|---|---|---|---|
| `device_id` | yes | string (GUIDv7) | Provenance proof. Minted by `newfirefly.ps1`, embedded in firmware build, never changes per-device. Its presence + parseable shape is *the* signal that this is a Zen Garden device. |
| `family` | yes | string | Always `"firefly"` for this protocol. Reserved for future Zen Garden hardware families. |
| `variant` | yes | string | One of `matrix` / `oled` / `tdisplay`. Adapter selectors match against this. |
| `version` | yes | string (semver) | Firmware version. Adapter predicates can use semver-caret matching. |
| `processor` | yes | string | `esp8266` / `esp32` / `rp2040`. Diagnostic + adapter-side capability hints. |
| `hardware_id` | recommended | string | Chip-unique identifier (ESP8266 chipId, ESP32 efuse MAC, RP2040 board ID). Forensic field — detects "same `device_id`, different physical board." |
| `display` | required for visual variants | object | `resolution` (e.g. `"128x64"`, `"5x5"`, `"135x240"`) + `type` (free-form descriptor). |
| `capabilities` | yes | string array | What protocol features this firmware supports. See §Capability registry. |

**Schema evolution policy**: additive-only. New optional fields ship without a schema-version bump (consumers ignore unknown fields). Renaming a field, repurposing a value, or removing a field bumps a `"schema": 2` field that current consumers do not require but future consumers respect.

### 2. HELLO frame — emitted unsolicited on boot

Firmware emits the descriptor immediately after boot, framed as:

```
* HELLO,{"device_id":"01938abc-…","family":"firefly",…}\n
```

Leading `* ` distinguishes the HELLO from in-band command output (which all start with `OK,…` or `ERR,…`). The bus listens for HELLO with a 3-second timeout after opening the port; if it arrives, identification is done — no `I` sent, no command parser needed yet, works mid-boot. **The ESP32 auto-reset on port open becomes a feature**: bus is waiting for HELLO during the reset window, not racing it.

### 3. `I` command — the fallback

For backward-compatibility within the firmware lifetime (e.g. firmware that boots faster than the host can listen) and for explicit re-query, `I` returns the same descriptor JSON wrapped in `OK,`:

```
> I
< OK,{"device_id":"01938abc-…","family":"firefly",…}
```

Bus sends `I` only if no HELLO arrived within the 3-second window after port open. Eliminates the active-probe path on healthy boots; provides a fallback for cold-boot timing edge cases.

### 4. Provenance — `newfirefly.ps1` mints, firmware embeds

Identity flow:

```
newfirefly.ps1 (host)
  1. Detect attached device + variant
  2. Mint GUIDv7 (PowerShell helper; .NET 9 has Guid.CreateVersion7(),
     earlier versions use a small inline implementation)
  3. Prompt operator for an optional human label ("garage-fountain")
  4. Generate firmware/include/device_id.h:
        #define DEVICE_ID "01938abc-de01-7234-89ab-cdef01234567"
  5. Build firmware (per-device build artifact)
  6. Flash firmware
  7. Append entry to ~/.zen-garden/firefly-roster.json
  8. Verify HELLO frame round-trips with the new device_id
```

Firmware emits `DEVICE_ID` from the generated header in every HELLO and `I` response. The header is the single source of truth on the device side; nothing else — no NVS write, no first-boot generation, no device-side RNG path.

### 5. Roster file — `~/.zen-garden/firefly-roster.json`

Host-side inventory written by `newfirefly.ps1`:

```json
{
  "version": 1,
  "fireflies": [
    {
      "device_id": "01938abc-de01-7234-89ab-cdef01234567",
      "minted_at": "2026-04-14T15:30:00Z",
      "minted_by": "leo@workstation",
      "label": "garage-fountain",
      "variant": "oled",
      "firmware_version_at_provisioning": "0.2.0",
      "stone_assigned_to": "stone-coral-prairie"
    }
  ]
}
```

Operator backs up this file alongside the rest of their Zen Garden state. Lost roster = lost labels and provenance metadata, not lost devices (devices keep working; their `device_id` simply lacks human-readable annotation until the roster is rebuilt by re-running `newfirefly.ps1` on each).

**Roster sync to stones** (Phase 1): manual via `garden-rake firefly roster push <stone>`. Stone caches it under `/var/lib/zen-garden/firefly-roster.json`; moss reads it on startup + on file-change watch. Garden-wide replication via the pond is Phase 2 work.

### 6. Lenient-by-default trust mode

The bus's posture toward unknown `device_id` values:

- **Lenient (default)**: any well-formed descriptor with a parseable `device_id` claims an adapter. Roster supplies labels and metadata when present but is not required for adoption. Suits the local-environment-now reality — operator's freshly-minted devices Just Work™ without per-stone roster sync.
- **Strict (opt-in)**: `--strict-roster` daemon flag. Bus rejects descriptors whose `device_id` is not in the synced roster. Suits future multi-operator gardens or hostile-network deployments.

### 7. `--allow-unprovisioned` dev-flash escape hatch

For firmware development without re-running `newfirefly.ps1`:

- Default: descriptor missing `device_id` → bus emits `core.companion.device.unprovisioned` telemetry, marks port as unclaimed, backs off.
- With `garden-firefly --allow-unprovisioned`: bus synthesizes a transient `device_id` from `dev-{hardware_id}-{boot_count}`, adapter spawns normally. Brightness etc. persist by hardware_id rather than device_id. Explicit, gated, non-default.

### 8. Capability registry (Phase 1)

Capability strings the daemon recognizes. Adapter predicates use `Pred::has_capability("...")` to gate use of optional protocol fields.

| Capability | Variants | Meaning |
|---|---|---|
| `dashboard` | oled-v2 | Accepts the `D,…` packed dashboard frame |
| `wipe-animations` | oled-v1, oled-v2 | Accepts `WIPE-IN,…` / `WIPE-OUT,…` |
| `brightness` | all displays | Accepts `B,<percent>` |
| `seed-bank-icon` | oled-v2 | Renders the seed-bank slot in dashboard |
| `gpu-bar` | oled-v2 (future), tdisplay | Renders the GPU utilization bar |
| `json-push` | tdisplay | Accepts `J,<json>` full state push |
| `load-incremental` | tdisplay | Accepts `L,…` partial-load updates |
| `service-deltas` | tdisplay | Accepts `+,<svc>` / `-,<svc>` |
| `pixel-control` | matrix | Accepts `P,<x>,<y>,<r>,<g>,<b>` and `F,<r>,<g>,<b>` |
| `animation-engine` | matrix | Runs host-driven animation frames |

Firmware updates can advertise new capability strings; adapter code checks at command-issue time. New capability + same adapter = forward-compat without per-version adapter forks.

## Implementation plan

**Chapter 1** — Schema spec doc + `newfirefly.ps1` rewrite:
- New `docs/specs/firefly-device-protocol.md` carrying the schema, HELLO frame format, capability registry, roster file format.
- Rewrite `installer/newfirefly.ps1`: mint GUIDv7, prompt label, generate `device_id.h`, append roster.

**Chapter 2** — Firmware updates (one variant per chapter increment):
- Each firmware variant gains a `device_id.h` include slot, emits HELLO on boot, accepts `I` returning the same descriptor.
- Land per-variant: oled-v1 (Ch2a), oled-v2 (Ch2b), tdisplay (Ch2c), matrix (Ch2d). Each is a small focused commit.

**Chapter 3** — Bus integration (depends on COMPANION-0012 Ch4 landing first):
- Implement `FireflyIdentityProtocol` against the device bus's `IdentityProtocol` trait.
- Replace the four firefly factories with `AdapterRegistration`s using descriptor predicates.
- Roster lookup integration: moss reads the synced roster; descriptors with known `device_id` get labels in telemetry.

**Chapter 4** — Operator tooling:
- `garden-rake firefly inventory` — lists all known fireflies (claimed status, label, last-seen, firmware version) by joining roster + bus telemetry.
- `garden-rake firefly roster push <stone>` — sync local roster to a stone.
- Documentation pass: provisioning runbook, troubleshooting guide ("device shows as unprovisioned" / "device shows as foreign" / "device shows as unclaimed").

Each chapter ships green to `dev`. The local environment reflashes per chapter — operator runs `newfirefly.ps1` on each device once Chapter 2's variant lands. Brief windows where mixed firmware exists are handled by lenient trust mode (older firmware emits CSV → bus identity protocol returns `None` → device classified as foreign → operator runs `newfirefly.ps1` → next attach uses HELLO).

## Out of scope (deferred)

| Item | Deferred |
|---|---|
| Backward-compat with the legacy CSV `I` response | Local environment only; `newfirefly.ps1` rollout reflashes everything within the chapter |
| Cryptographic signing of `device_id` | Local trust model; revisit if external users land |
| Operator-set labels stored on-device | Roster file is the label authority; on-device storage adds NVS lifecycle complexity |
| Garden-wide pond-replicated roster | Phase 2 of operator tooling; Phase 1 is manual `rake firefly roster push` |
| `device_id` rotation without reflash | Re-run `newfirefly.ps1` mints a fresh GUID; rotation = reflash by design |
| HELLO frame in CBOR / msgpack instead of JSON | Bytes saved are not worth the operator-debuggability loss |

## References

- [COMPANION-0012](COMPANION-0012-device-bus.md) — the bus that consumes this protocol
- [FIREFLY-0001](FIREFLY-0001-v0-implementation.md) — the original CSV-based protocol this supersedes
- [FIREFLY-0002](FIREFLY-0002-esp8266-oled-device.md) — OLED v1 protocol
- [FIREFLY-0003](FIREFLY-0003-tdisplay-diorama.md) — T-Display protocol
- [RFC 9562 §5.7](https://www.rfc-editor.org/rfc/rfc9562#section-5.7) — UUIDv7 spec
