# The fleet's knowledge base — machine facts, access, and hard-won quirks

Living document. Every machine we touch gets an entry here: identity,
access path, hardware facts, and the quirks that cost us time (so they
never cost us time twice). This is also the raw material for the
public-release install story: **"delightfully easy on any system —
Linux, Windows, Apple"** is a charter requirement (see
`docs/v1/design/install-delight.md`), and every quirk below is a
bubble the installer must pop for the user.

## Access law (all test hardware)

- Credentials: `test/test` (user `test`, wheel/sudo). Older stones
  use `stone/stone`; when in doubt try `test`, then `stone`.
- The workstation's key (`~/.ssh/id_ed25519`) is installed on every
  cooperating machine; BatchMode SSH from the workstation is the norm.
- First contact on a new box (no key yet): `plink -ssh -pw test`,
  with `-hostkey SHA256:...` after the first fingerprint shows —
  OpenSSH on Git Bash has no sshpass, and plink's prompt hangs pipes.

## The stones

### test-02 — the new Arch machine (2026-08-30)

| Fact | Value |
|---|---|
| Hostname | `test-02` (resolves via mDNS `test-02.internal`) |
| Address | `192.168.1.95` (DHCP, iface `enp0s31f6`) |
| MAC | `70:4d:7b:84:76:73` |
| OS | Omarchy (Arch-based; `PRETTY_NAME="Omarchy"`) |
| Kernel | Linux 7.1.9-arch1-2, x86_64 |
| CPU | Intel Core i5-7400 @ 3.00GHz, 4 cores |
| RAM | 15 GiB |
| Disk | 464 GB root (441 free) |
| Docker | 29.7.2 — works passwordless after group add (below) |
| User | `test` (uid 1000, wheel) |
| SSH host key | ed25519 `SHA256:hYaztj0GANITCjatrQStLI4LO906z5IFKoLM7DTf1KQ` |

**In the garden since 2026-08-31 (W18):** `stone-crimson-estuary`
(id 01a055a6-159b…), installed via the npm tarball (`npm i -g
zen-garden-0.1.0.tgz && zen install`); moss at `~/.zen-garden/bin`,
journal flowing. Node is mise-managed (`mise reshim` after global
installs!). Installed by `zen`, controlled by `zen up|down|status`.

**Quirks found (fresh-install reality):**
0. **Omarchy ships ufw ACTIVE** — the room is deaf/blind until
   `sudo ufw allow 7284:7299/udp && sudo ufw allow 7285/tcp`
   (`zen doctor` detects + says this). Probe firewall state with
   `systemctl is-active ufw` — `ufw status` needs root and dies
   quietly for non-root.
1. **Fresh Arch ships no sshd running** — `pacman -Sy openssh &&
   systemctl enable --now sshd` is a console step before any remote
   work. (Installer lesson: the install script must detect/offer SSH
   setup, or the garden's remote stone flow is dead on arrival.)
2. `sudo` needs the password (no NOPASSWD) — pipe with
   `echo test | sudo -S`.
3. Docker installed but **user not in `docker` group** —
   `usermod -aG docker test`, then RE-LOGIN (new SSH session) before
   passwordless docker. A garden installer must handle both the group
   add and the session-refresh, or check-and-guide.
4. `systemctl is-active docker` said `inactive` while
   `docker version` answered 29.7.2 — socket activation; don't trust
   is-active alone as the daemon probe.

### stone-tranquil-pass (.195) — the settled stone

NOTE (W18): a PoC-era `/usr/local/bin/garden-moss` SERVICE still runs
here beside the v1 moss (PID-class ~2475, systemd). Harmless so far
(both hear UDP 7284); retire deliberately, not by accident.

| Fact | Value |
|---|---|
| Address | `192.168.1.195` |
| OS | Debian 13, x86_64 |
| Role | sink holder (USB seed bank `seed-vault::default` at
| | `/mnt/gposingway-seed`, 238.5 GiB), replant host |
| Deploy | `~/zen-v1/{moss,rake}`; `MOSS_RUNTIME=docker setsid nohup ./moss` |
| Quirks | key auth works; `pkill -INT -f '^\./moss$'` = graceful stop |

### stone-entry-glass — the workstation (this machine)

| Fact | Value |
|---|---|
| Address | `192.168.1.137` |
| OS | Windows 11 (win32 10.0.26200), Git Bash + Docker Desktop |
| Build path | repo `src/v1/target/release/` (firewall-trusted); the
| | temp build path has explicit Block rules — see W15 P2 |
| Quirks | taskkill //F //IM moss.exe = the murder; Windows firewall
| | must allow the ACTUAL binary path (UDP 7284-7299, TCP 7285) |

### stone-translucent-clearing (.82) — the bystander

Old PoC build, chirps only, no v1 API, no SSH key installed
(`stone@` denied). Useful as the live cross-generation tolerance case;
upgrade path TBD.

### stone-crystalline-dune (.111)

Was down at W15; now answers ping but SSH key auth fails. Deploy the
key to bring it back as the fourth stone.

## Hard-won cross-machine lessons (install-relevant)

- **Windows firewall keys off the binary's full path** — a rebuild in
  a new directory silently breaks the room. The installer must add
  rules for the installed location (and the upgrader must handle a
  path change).
- **Linux deploy ritual** (worked on .195, applies to test-02):
  cross-build in `rust:latest` docker, `scp` as `.new`, `mv` over,
  never overwrite a running binary; restart with setsid/nohup and all
  streams redirected.
- **SSH one-liners with background jobs** hold the connection open
  until ALL streams close — redirect everything to files.
- **Docker group membership needs a fresh session** — remote
  provisioners must re-connect before testing docker.
- **Fresh installs lack sshd** (Arch), and distros differ in daemon
  probing — check the API, not the service state.
