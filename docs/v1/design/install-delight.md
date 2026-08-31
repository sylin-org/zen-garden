# Design note — Install delight (public-release requirement)

**Status:** requirement recorded 2026-08-30 (operator direction).
**Goal:** when zen-garden goes public, installing it on ANY system —
Linux, Windows, Apple — must be *delightfully easy*. One command (or
one download-and-double-click), a garden that wakes up and sings, zero
archaeology.

This note is the requirement's home; `docs/v1/inventory/fleet-facts.md`
is the evidence base — every quirk we hit on real hardware is a bubble
the installer must pop for the user.

## The law of the first five minutes

A new user's machine, five minutes after deciding to try zen-garden:

1. The moss is running, singing to the room, visible on `rake observe`.
2. Docker was found — or its absence was said plainly, with the exact
   fix for THEIR system (not a link, the fix).
3. The firewall was handled for them (Windows path rules, Linux
   group+session, macOS helper prompt).
4. A stone identity exists; the garden has a name they chose.
5. `rake offer <something small>` works, and they can see it in a
   browser or on the wall.

Anything beyond that is our problem, not theirs.

## The bubbles we must pop (evidence: fleet-facts.md)

| # | Bubble | Where found | The installer must... |
|---|---|---|---|
| 1 | No sshd on fresh installs | test-02 (Arch) | detect+offer, or run local-only |
| 2 | Docker not installed / daemon down | any fresh box | detect, guide per-OS, plain words |
| 3 | User not in docker group | test-02 | add + tell the user a new session is needed (and survive remote provisioning) |
| 4 | Windows firewall blocks the binary BY PATH | entry-glass | add rules for the installed path; handle upgrade path changes |
| 5 | Binary-in-use cannot be replaced | .195 deploy ritual | atomic swap (.new → mv), never over the running image |
| 6 | Daemon probes lie (socket activation) | test-02 | probe the API, not systemctl |
| 7 | Static musl/glibc drama (future Linux) | — | static build or bundled runtime; test on Arch + Debian at minimum |
| 8 | Apple notarization/Gatekeeper | — (no Mac in fleet yet) | sign + notarize, or document the right-click-open path honestly |

## Shape (proposal, for the release epic)

- **One static binary per OS** (moss embeds rake's functions? or ship
  two): `curl -sL zen.garden/install | sh` on Linux/macOS;
  `winget install zen-garden` / a signed `.msi` on Windows.
- **The installer is a moss verb**: `moss doctor` / `rake doctor` —
  checks docker, firewall, ports, disk, and says what to do in plain
  language per-OS. Same code path the install script uses, so the
  doctor IS the installer's conscience.
- **Identity onboarding**: first run asks for a stone name (or mints
  one), writes `~/.zen-garden/identity.json`, sings immediately.
- The catalog's 51 manifests ship embedded (already true) — day-one
  value with zero network beyond docker pulls.

## Fleet coverage today (install-test matrix)

| OS | Machine | Status |
|---|---|---|
| Arch (Omarchy) | test-02 | provisioned 2026-08-30; docker OK; moss NOT yet deployed |
| Debian 13 | .195 | full citizen (the settled stone) |
| Windows 11 | workstation | full citizen (entry-glass) |
| Old PoC Linux | .82 | tolerance case; upgrade TBD |
| Apple | — | **gap — we need a Mac before release** |
