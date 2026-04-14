---
audience: [developer, operator]
doc_type: spec
status: current
last_verified: 2026-04-14
---

# Firefly Device Protocol

The serial contract every firefly device speaks. Formalized in [FIREFLY-0004](../decisions/FIREFLY-0004-device-protocol.md); consumed by [COMPANION-0012](../decisions/COMPANION-0012-device-bus.md)'s device bus.

---

## Summary

Every firefly device — matrix, oled-v1, oled-v2, tdisplay — speaks one identification protocol. The daemon's device bus opens the serial port, waits briefly for the device to announce itself, and parses a JSON descriptor that carries the device's provisioned identity, firmware metadata, and capability list.

---

## Wire frame

### HELLO (unsolicited, emitted on boot)

```
* HELLO,{<descriptor-json>}\n
```

- Leading `* ` distinguishes from in-band command output (which uses `OK,…` / `ERR,…`).
- `<descriptor-json>` is the single-line JSON object described below.
- `\n` terminates.

### `I` response (host-initiated fallback)

```
> I\n
< OK,{<descriptor-json>}\n
```

Same descriptor body, wrapped in the standard `OK,…` success prefix. Used when the host missed the HELLO (e.g. opened the port too late in the device's boot cycle).

### Bus behaviour

1. Bus opens the port. ESP devices auto-reset on port open — this is intentional, it synchronizes the boot with the bus.
2. Bus reads with a 3-second timeout.
3. If a line starting with `* HELLO,` arrives, parse and identify.
4. Otherwise, send `I\n`, read one reply. If it starts with `OK,{`, parse and identify.
5. Otherwise, the device is classified as foreign / unresponsive and enters backoff.

---

## Descriptor schema

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

### Required fields

| Field | Type | Notes |
|---|---|---|
| `device_id` | string (GUIDv7) | Minted by `newfirefly.ps1`, embedded in firmware. Presence and parseable shape is the provenance signal — only devices that have been through `newfirefly.ps1` can emit a valid one. |
| `family` | string | Always `"firefly"` for this protocol. Reserved for future Zen Garden hardware families. |
| `variant` | string | One of `matrix`, `oled`, `tdisplay`. Disambiguates firmware surface within the firefly family. |
| `version` | string (semver) | Firmware version. Bus adapter predicates use semver-caret matching (`^0.2.0`). |
| `processor` | string | One of `esp8266`, `esp32`, `rp2040`. Diagnostic; may influence capability inference. |
| `capabilities` | string array | See [capability registry](#capability-registry). |

### Recommended fields

| Field | Type | Notes |
|---|---|---|
| `hardware_id` | string | Chip-unique identifier (ESP8266 chipId, ESP32 efuse MAC, RP2040 board id). Forensic — detects "same device_id, different physical board." |
| `display` | object | `{ "resolution": "<WxH>", "type": "<free-form>" }` for visual variants. Absent for matrix (no display). |

### Schema evolution

**Additive-only** changes don't bump any version field — consumers ignore unknown fields.
**Breaking** changes bump a top-level `"schema": 2` field that current consumers do not require but future ones respect.

---

## Capability registry

Capability strings the daemon recognizes. Adapter code checks `has_capability("…")` before issuing optional protocol commands — this lets firmware roll out new features without forking the adapter per version.

| Capability | Variants | Meaning |
|---|---|---|
| `dashboard` | oled-v2 | Accepts the `D,<cpu>,<mem>,<disk>,<uptime>,<offerings>,<stones>,<net>,<seed_bank>` packed frame. |
| `wipe-animations` | oled-v1, oled-v2 | Accepts `WIPE-IN,<line1>,<line2>` and `WIPE-OUT,<line1>,<line2>`. |
| `brightness` | all displays | Accepts `B,<0-100>`. |
| `seed-bank-icon` | oled-v2 | Renders a seed-bank presence slot in the dashboard. |
| `gpu-bar` | oled-v2 (future), tdisplay | Renders a GPU utilization bar. |
| `json-push` | tdisplay | Accepts `J,<compact-json>` full-state push. |
| `load-incremental` | tdisplay | Accepts `L,<cpu>,<mem>,<disk>,<io>,<gpu>,<gpu_active>`. |
| `service-deltas` | tdisplay | Accepts `+,<svc>,<health>` and `-,<svc>`. |
| `pixel-control` | matrix | Accepts `P,<x>,<y>,<r>,<g>,<b>` and `F,<r>,<g>,<b>`. |
| `animation-engine` | matrix | Accepts host-driven frame updates. |

New capabilities land by updating the firmware's descriptor and adding a matching check in the adapter. No protocol version bump required.

---

## Provisioning ritual

Running `newfirefly.ps1` against a connected device performs:

1. **Detect** the attached device + variant.
2. **Mint** a GUIDv7 via the PowerShell helper.
3. **Prompt** for an optional human label (stored in the roster, not on the device).
4. **Embed** the GUID. MicroPython devices get a `/device_id.txt` file uploaded; CircuitPython devices get the same file copied to the `CIRCUITPY` drive.
5. **Flash / reset** firmware so the new file is picked up on the next boot.
6. **Append** an entry to `~/.zen-garden/firefly-roster.json` (host-side inventory).
7. **Verify** by reading the first HELLO frame and confirming the returned `device_id` matches what was minted.

Re-running `newfirefly.ps1` against the same device mints a fresh GUID by design. To keep an existing identity across firmware updates, operators re-upload the same `device_id.txt` — which is exactly what the roster preserves.

---

## Roster file

Host-side inventory at `~/.zen-garden/firefly-roster.json`:

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

- Backed up alongside operator state. Loss = loss of labels and provenance history, not loss of devices.
- Synced to stones via `garden-rake firefly roster push <stone>` (tooling lands in [FIREFLY-0004 Chapter 4](../decisions/FIREFLY-0004-device-protocol.md)).
- Moss reads the synced copy for telemetry + identity lookup.

---

## Trust modes

The daemon exposes two postures toward descriptors:

- **Lenient (default)** — any well-formed descriptor with a parseable `device_id` can be claimed by a matching registration. Roster supplies labels if present but is not required. Suits local-environment operation.
- **Strict (`--strict-roster`)** — descriptors whose `device_id` is not in the synced roster are rejected. Suits future multi-operator gardens.

Switch between them via the daemon's CLI.

---

## Development workflow

For firmware work where re-running `newfirefly.ps1` per rebuild is friction:

- `garden-firefly --allow-unprovisioned` accepts devices emitting descriptors without a `device_id`. A synthetic dev-mode identity (`dev-<hardware_id>-<boot_count>`) is fabricated; adapter state persists by hardware_id instead.
- Not the default. Explicit flag. Not for production.

---

## References

- [FIREFLY-0004](../decisions/FIREFLY-0004-device-protocol.md) — the decision record.
- [COMPANION-0012](../decisions/COMPANION-0012-device-bus.md) — the device bus that consumes the protocol.
- [RFC 9562 §5.7](https://www.rfc-editor.org/rfc/rfc9562#section-5.7) — UUIDv7 specification.
