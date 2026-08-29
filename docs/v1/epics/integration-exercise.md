# EPIC — The Full Integration Exercise (W15)

**Status:** READY TO RUN — written 2026-08-29, for a fresh session.
**Purpose:** exercise EVERYTHING we built, across machines, as one story:
two stones meet, work is planted, files cross the network, a living will
ferries across stones, a stone is murdered, its work is replanted from
the seed bank, and a goodbye is witnessed live. Everything recorded as
**W15** in `src/v1/WITNESSES.md`.

**The plan is approved by the operator** (amendment: the goodbye is a
graceful restart of a stone — SIGINT — watched from the other side).

---

## The cast

| Stone | What it is | Role |
|---|---|---|
| **stone-tranquil-pass** (192.168.1.195) | Linux, Debian 13, Docker, **USB seed bank mounted** (`seed-vault::default`, roles: sink, `/mnt/gposingway-seed`) | the settled stone: holds the sink, receives ferries, does the replanting, speaks the goodbye |
| **workstation entry-glass** (192.168.1.137, this Windows machine) | native Windows moss + Docker Desktop | the young stone: boots fresh, plants ntfy, gets murdered |
| .82 translucent-clearing | chirps only (old build, no SSH key) | bystander |
| .111 crystalline-dune | down | absent |

## Rules of engagement

1. **Leave the fleet as found.** .195 ends with only `witness-db::garden`;
   the workstation's moss ends STOPPED; no test containers, images, or
   records survive.
2. **Record honestly.** Every phase gets a pass/fail line in W15. A
   failure is recorded, diagnosed, and either fixed forward or noted —
   never papered over.
3. Never run cargo from the repo root (it hits the PoC workspace).
4. The current build is ALREADY deployed on .195 (moss + rake, as of the
   MCP slice). Verify versions rather than redeploying blindly.

## Machine facts & hard-won gotchas (read twice)

- `.195`: `ssh -o BatchMode=yes stone@192.168.1.195`. Deploy ritual:
  `tar -cf /c/temp/zg-src.tar -C src/v1 --exclude=target .` **from
  `src/v1` or with `-C src/v1` from the root** (a root-cwd tar breaks the
  build: the root Cargo.toml pulls koi), then
  `docker run --rm -i -v "//c/temp/zg-src.tar:/src.tar" -v "//f/Replica/NAS/Files/repo/github/sylin-org/zen-garden/.cargo:/out" rust:latest bash -c "mkdir -p /src && tar -xf /src.tar -C /src && cd /src && cargo build --release -p garden-moss -p garden-rake && cp target/release/moss target/release/rake /out/"`,
  then `scp .cargo/{moss,rake} stone@…:/home/stone/zen-v1/{moss.new,rake.new}`
  (NEVER over the running binary), `mv` + `chmod +x`, restart with
  `pkill -u stone -f '^\./moss$'; sleep 2; MOSS_RUNTIME=docker setsid nohup ./moss >> moss.log 2>&1 < /dev/null &`.
  Mind `&`/`&&` precedence in ssh one-liners: a backgrounded `cd A && run`
  leaves the parent shell where it was.
- **Workstation moss**: build natively first
  (`cd src/v1 && CARGO_TARGET_DIR=/c/temp/zg-target cargo build --release -p garden-moss -p garden-rake`)
  → `C:\temp\zg-target\release\{moss.exe,rake.exe}`. Run it as a
  background Bash task: `MOSS_RUNTIME=docker MOSS_HTTP_PORT=7285 /c/temp/zg-target/release/moss.exe >> /c/temp/zg-moss.log 2>&1`.
  Its identity already exists (`~/.zen-garden`, entry-glass, 192.168.1.137).
  `rake.exe` is at the same release dir. `rake pulse` on a non-tty prints
  sequential `--- frame N ---` text blocks (witnessable from files).
- **Windows Firewall**: the moss binary path changed — if .195 cannot hear
  the workstation's chirps (P1 fails), allow inbound for
  `C:\temp\zg-target\release\moss.exe` (UDP 7284–7299, TCP 7285) via
  Windows Firewall settings, then restart the workstation moss.
- **Graceful goodbye** on .195: `pkill -INT -u stone -f '^\./moss$'`
  (SIGINT → the moss speaks goodbye and drains). The MURDER is
  `pkill -9` on the workstation moss (no goodbye — that is the point).
- **Root-owned leftovers** (D17): the uproot now asks the world to purge;
  if anything still resists,
  `docker run --rm -v <dir>:/og busybox sh -c 'rm -rf /og/...'`.
- **Timeouts over ssh**: `timeout 20 ./rake pulse > file` works; ssh
  one-liners with background jobs keep the ssh open until ALL streams
  close — redirect everything to files.

## The phases

### P0 — Ground truth
1. `docker version` on the workstation (Docker Desktop up).
2. .195 runs the current build (it does; verify: `~/zen-v1/rake observe`
   answers, `curl -s localhost:7285/mcp -X POST -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | head -c 120`
   answers tools).
3. The seed bank: `~/zen-v1/rake storage` on .195 lists
   `seed-vault  mounted  /mnt/gposingway-seed`. If absent: mount the USB
   (`sudo mount /dev/sdb2 /mnt/gposingway-seed` — the operator's console
   may be needed) and re-adopt if the moss does not recognize it.

### P1 — The room meets
1. Start the workstation moss (background task, log to a file).
2. Within ~15s: `rake.exe observe` lists BOTH stones
   (entry-glass + tranquil-pass). Also verify from .195's side.
**Pass:** both stones, both thriving, no heartbeat waited.

### P2 — Life on the young stone
1. `rake.exe offer ntfy` (catalog manifest: has a data volume and a
   living will from D15).
2. `rake.exe list` shows ntfy running with a ledgered port; .195's
   `garden/stones` view shows the offering under entry-glass.
**Pass:** running, visible from BOTH sides.

### P3 — Capabilities and the wish
1. On .195: `~/zen-v1/rake offer ollama` (pulls the image — be patient).
2. `~/zen-v1/rake ensure 'ollama[model:all-minilm]'`.
**Pass:** "grown, not planted" with the connection string;
`rake capabilities ollama` shows the model.

### P4 — The cross-stone file write (the headline)
1. From the WORKSTATION:
   `rake.exe files seed-vault put zg-integration/hello.txt` (check
   `rake files --help` for exact syntax; feed the file content
   "the garden routes").
2. The workstation's face does NOT hold the bank — it must answer the
   garden's redirect and the client follows it to .195.
3. Verify on .195:
   `find /mnt/gposingway-seed -name hello.txt` + `cat` it.
4. Read it back through the workstation
   (`rake.exe files seed-vault get zg-integration/hello.txt`),
   move it (`mv`), read again, delete it.
**Pass:** byte-identical round trip through a machine that does not
hold the drive.

### P5 — The living will, cross-stone
1. On the WORKSTATION: `rake.exe capture ntfy` — imprint (raw copy of
   the data volume, copy-freely per its D15 will) → pack → **ferry to
   seed-vault on .195** → commit.
2. Verify on .195: the checkpoint archive exists on the seed bank and
   `~/zen-v1/rake capture-last ntfy` (or the storage listing) shows it.
**Pass:** checkpoint committed on the OTHER stone's drive.

### P6 — The murder
1. `taskkill /F /IM moss.exe` on the workstation (no goodbye — a real
   death) and `docker rm -f` the ntfy container.
2. Wait ~2 minutes. From .195: `rake observe` shows entry-glass
   **expired** (silent past the threshold) and its offerings gone.
**Pass:** honest expiry after silence, not a haunting.

### P7 — The replant
1. On .195: `~/zen-v1/rake replant ntfy` — restore from the checkpoint
   the dead stone ferried.
2. `rake list` shows ntfy running ON .195; the data volume carries the
   dead stone's state; `~/zen-v1/rake jobs` /
   the audit chain shows `Replanted`.
**Pass:** same FQN, running on the survivor, data intact.

### P8 — The room re-serves the dead stone's work
1. From the WORKSTATION: `rake.exe ensure ntfy` answers where ntfy now
   lives (.195) — the wish routes to the replant.
2. MCP: `curl -s -X POST http://192.168.1.195:7285/mcp -H 'content-type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"observe","arguments":{}}}'`
   shows ntfy under tranquil-pass.
**Pass:** wishes and MCP agree with reality.

### P9 — The goodbye (graceful, at last)
1. The .195 moss is restarted fresh (so the wall is quiet): restart it
   per the deploy ritual (no binary change).
2. On the WORKSTATION: `rake.exe pulse > /c/temp/zg-goodbye.txt` in the
   background (non-tty frames).
3. On .195: `pkill -INT -u stone -f '^\./moss$'` — graceful SIGINT.
4. Watch the workstation's capture: a goodbye event for
   tranquil-pass; its garden-strip row leaves immediately (no
   threshold wait).
**Pass:** goodbye spoken, room updated instantly. (This closes W12's
honest note.)

### P10 — Return, cleanup, record
1. Restart the .195 moss → seen again by the workstation's feed.
2. CLEANUP, in order: on .195 `rake uproot ntfy` and `rake uproot ollama`,
   `docker rmi` their images, remove any offering directories the
   uid-0 purger missed (busybox trick if needed), clear
   `~/.zen-garden/journal`; verify `rake list` shows only
   `witness-db::garden`. On the workstation: stop the moss task,
   remove `~/.zen-garden/offerings/ntfy`, `docker rmi` ntfy's image.
3. Write **W15 — the full integration exercise** in
   `src/v1/WITNESSES.md`: one line per phase (pass/fail + evidence),
   the finds, the state left behind.
4. Commit and push.

## If something fails

Record the failure in W15 exactly as witnessed, then either fix forward
(a fix commit is part of the epic) or note it as DEBT with a named
gate. A phase may be re-run after a fix; the story order matters —
P5's ferry must precede P7's replant.

## Definition of done

All ten phases green (or honestly recorded with follow-ups), W15
written, the fleet as found, pushed to origin/dev.
