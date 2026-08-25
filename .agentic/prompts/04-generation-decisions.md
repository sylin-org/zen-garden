# 04 — Generation Decisions: Park, Succeed, Rename

> Resolve the three "two-things-claiming-one-name" situations: ai-vs-ollama, pavilion, lantern. ~49k lines
> leave the active tree; every crate label becomes true. Phase: Subtract. Depends on: 03. Blocks: 06, 08.

## Mission

Three subsystems carry an identity problem that costs every future reader:

1. **Two AI-orchestrator generations coexist** (57k lines, ~20% of all project Rust) with no succession
   statement: `ollama` (deployed, published, documented) and `ai` (newer design, operationally dormant).
2. **Pavilion** (Windows Tauri client, 6.8k Rust + 4.6k TS) sits mid-milestone with its flagship feature
   admittedly broken end-to-end, idle since 2026-05-06, silently implying active development.
3. **Lantern** presents as the "registry recommended beyond 20 stones" while its registry design
   (LANTERN-0001: SQLite, election, HA) is not in the code — what shipped is a good topology dashboard.

Execute the succession/parking with ADRs so the decisions are durable, then make every doc surface agree.

## Ground truth (verified 2026-06-11 — re-verify each)

| Fact | Re-verify |
|---|---|
| `src/orchestrators/ai`: 41,892 Rust lines (204 files; 57,557 incl. dashboard); last commit touching it (all branches) 2026-04-12 | `git log --all --oneline -1 -- src/orchestrators/ai` |
| ai is absent from `installer/build-orchestrators.ps1`; no Docker Hub push script (only local `start.bat`); never registers with the Moss gateway/mDNS; absent from `.agentic/CONTEXT.md` | `grep -n "ai" installer/build-orchestrators.ps1; ls src/orchestrators/ai/*.bat` |
| ollama: 14,809 Rust lines; in the build script; Hub push exists (`sylinorg/zen-garden-ollama-orchestrator`); documented in `docs/guides/ollama-orchestrator.md` + `.agentic/reference/api-endpoints.md` | `grep -rn "ollama" installer/build-orchestrators.ps1` |
| Nuance: ollama's last **substantive code** change is 2026-03-25 (commit 6a219f28) — *older* than ai's last work; the live/dormant contrast is operational (deployment/docs/announce), not code-age | `git log --oneline -5 -- src/orchestrators/ollama/src` |
| ai has the best test density of the area (396 test annotations) and 3 real bugs: unawaited `events.publish` in its flow_executor (~lines 452/476/486) | `grep -rn "events.publish" src/orchestrators/ai/src --include="*.rs" \| head` |
| ai's founding ADRs ORCH-0013/0028/0029/0030 are still status `proposed` | `grep -l "status.*proposed" docs/decisions/ORCH-0013* docs/decisions/ORCH-0028* docs/decisions/ORCH-0030*` |
| Pavilion: PAVILION-0002 admits the M1 Cloud Filter upload doesn't work end-to-end; idle since 2026-05-06 | `git log --oneline -1 -- src/pavilion` |
| Lantern: zero hits for election/sqlite/blake3 in `src/lantern` (38 .rs files); a one-day BLAKE3 election prototype existed 2026-01-24 and was deleted 2026-01-25 (commit 7ed41af4); what lives: passive mDNS browse, opt-in heartbeat (`LANTERN_ENDPOINT`-gated), 15s Moss polling, `GET /api/v1/resolve`, SSE, React dashboard | `grep -rin "election\|sqlite\|blake3" src/lantern/src \| wc -l` (expect 0) |
| README.md:41 still says "Lantern registry recommended beyond 20" | `grep -n "Lantern" README.md` |

## Research first (~45 min)

1. Read `docs/decisions/ORCH-0013` (adapter promotion + the reverted first attempt) and ORCH-0028/0030
   (the "break and rebuild" that produced ai) — you will write the succession ADR as their closure.
2. Skim `src/orchestrators/ai/src/` top-level module tree and its EventBus + Resources (GPU claim
   accounting) designs — the ADR must name what is worth harvesting later.
3. Read `docs/decisions/LANTERN-0001` and `src/lantern/src/main.rs` to write the lantern reframe honestly
   (which LANTERN-0001 surface DID land: TTL contract, resolve, SSE).

## Plan gate — OPERATOR decisions (present, then stop until answered)

1. **Succession**: recommend *ollama is the present; ai is archived* (it is the deployed, published,
   documented generation; the strategy assessment concurs). Alternative — promoting ai — requires: adding
   it to build scripts, gateway registration, Hub push, docs, and fixing its 3 unawaited publishes; that
   is a different, larger prompt. Present both; default to archive-ai if the operator has no preference.
2. **Park destination for ai and pavilion**: archive branch in-repo (`archive/ai-orchestrator`,
   `archive/pavilion`) vs sibling repos. Recommend archive branches (zero infra, history intact).
3. **Lantern rename**: keep crate name `garden-lantern` but reposition as "garden dashboard" in all docs
   (cheap), vs rename the crate too (touches installer + ports docs). Recommend docs-only now.

## Target shape

Succession ADR `docs/decisions/ORCH-0042-orchestrator-succession.md` (next free ORCH number — verify):

```markdown
# ORCH-0042: Ollama orchestrator is the maintained generation; ai crate archived

Status: accepted (2026-XX-XX)

## Decision
The `ollama` orchestrator is the maintained AI-placement generation. The `ai` crate (ORCH-0028/0030
rebuild) is archived to branch `archive/ai-orchestrator` pending real-user demand (staying-focused.md
trigger). Its designs worth harvesting: unified EventBus (src/.../events), GPU claim-accounting
(Resources domain), capability-router API shape.

## Consequences
- ORCH-0013, ORCH-0028, ORCH-0029, ORCH-0030 → status: superseded (this ADR).
- ADRs 0031/0034/0036/0038 describe archived functionality; marked accordingly.
- build-orchestrators.ps1 and CI matrices reference {ollama, mongodb, common} only.
```

Pavilion gets a status note, not an ADR: `src/pavilion/STATUS.md` (5 lines: parked date, why, what works,
what doesn't per PAVILION-0002, where the archive branch is). Same pattern if ai stays in-tree is rejected.

Lantern reframe: README table row becomes
`**Lantern** | Optional garden dashboard (topology, resolve, events) — registry features are roadmap`.

## Implementation

1. Create archive branches at current HEAD: `git branch archive/ai-orchestrator && git branch
   archive/pavilion` (branch creation only — deletion of the working-tree copies happens next; history
   plus branch pointer = recoverable forever).
2. `git rm -r src/orchestrators/ai`; remove ai from any CI matrix (added in prompt 02) and from
   `orchestrators/` docs references; commit `chore(orch): archive ai orchestrator generation (ORCH-0042)`.
3. `git rm -r src/pavilion` + remove from workspace members + drop pavilion-only koi dep lines if any;
   commit `chore(pavilion): park to archive/pavilion branch`. CAUTION: moss's `infra/cloud_filter` is
   already gone (prompt 03); verify nothing else imports pavilion (`grep -rn "pavilion" src --include="*.rs" --include="*.toml" | grep -v src/pavilion`).
4. Write ORCH-0042; flip the four ADR statuses; write `STATUS.md` content into the archive branches'
   READMEs if practical, else into the ADR.
5. Lantern: update README.md:41 wording, `docs/guides/` references, and add a paragraph to LANTERN-0001
   marking the three unbuilt pillars as `superseded: parked` with a pointer to what shipped. Remove
   lantern's unused `sqlx` dependency if present (`grep -n sqlx src/lantern/Cargo.toml`).
6. Update `.agentic/CONTEXT.md`'s module map to the new truth (this is the ONE doc-map edit in this
   prompt; the full doc sweep is prompt 08).

## Definition of done

- [ ] `src/orchestrators/ai` and `src/pavilion` absent from working tree; both archive branches exist and
      contain them (`git show archive/ai-orchestrator:src/orchestrators/ai/Cargo.toml | head -3`).
- [ ] `cargo check --workspace` green; ollama/mongodb/common orchestrators green.
- [ ] ORCH-0042 written; ORCH-0013/0028/0029/0030 statuses flipped; LANTERN-0001 annotated.
- [ ] `grep -rn "pavilion\|orchestrators/ai" src installer .github --include="*.toml" --include="*.ps1" --include="*.yml"` → only intentional archive references.
- [ ] README Lantern row updated. `.agentic/CONTEXT.md` module map matches the tree.
- [ ] Report line delta (expect ~−50k working-tree lines).

## Out of scope

Porting ollama onto orchestrator-common (prompt 13 bundles it with the storage/AI-surface work). Fixing
ai's bugs (it's archived). The broader docs sweep (08). Touching mongodb or ollama code.
