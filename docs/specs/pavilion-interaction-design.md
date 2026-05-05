---
audience: [contributor, designer, developer]
doc_type: spec
status: current
last_verified: 2026-05-04
canonical: true
note: "Living spec for Pavilion's visual and interaction language. Refined during implementation."
---

# Pavilion — Interaction Design Specification

**Purpose**: Establish the visual and interaction vocabulary for [Pavilion](../decisions/PAVILION-0001-windows-client-separation.md), the Windows tray client. This spec is *living* — it captures shared conventions but defers exact pixel decisions to the implementation pass.

**Audience**: Designers and developers building Pavilion's UX. Reference for anyone touching the dashboard, tray, or notification surfaces.

**Scope**: What Pavilion looks like and how it behaves in response to user input. Does not specify backend behaviour, OS integration internals, or Cloud Filter mechanics — those are in PAVILION-0001.

---

## 1. Design principles

These four principles are load-bearing. Every visual or interaction choice should trace back to one of them.

**Calm by default, present when needed.** Pavilion lives in the tray. It does not demand attention. The dashboard is opened deliberately. Toasts fire only for events the user genuinely cares about. The surface is restful when nothing is happening.

**Show health, not chrome.** Color, motion, and weight communicate the *state* of the garden — not decoration. Green is "all good"; amber is "needs attention"; red is "broken." Static panels in a healthy garden are visually quiet. Anomalies stand out without animation.

**Cascade-first; explicit when ambiguous.** This mirrors [URI-0003](../decisions/URI-0003-zen-garden-urn-form-scheme.md) at the UX layer. Users address resources by name first; the system resolves. Explicit kind selectors (offering vs stone vs bank) appear only when ambiguity is real.

**Direct manipulation over forms.** Where a gesture (drag, drop, click-and-hold) can express intent, prefer it over a multi-step form. The garden has structure — the UI should let users *touch* it.

---

## 2. Visual language

### Color system

The canonical token source is **[`src/lantern/frontend/src/tokens.css`](../../src/lantern/frontend/src/tokens.css)** — Pavilion adopts it verbatim to keep Lantern's web dashboard and Pavilion's Windows client visually consistent. CSS components reference these tokens; hard-coded hex values are forbidden.

| Token | Role | Typical use |
|---|---|---|
| `--bg`, `--bg2` | Window / panel backgrounds | Content area; sidebar; cards |
| `--s9` … `--s3` | Stone palette (light → dark) | Typography hierarchy; `--s9` for primary text, `--s4`–`--s5` for muted, `--s3` for scrollbar |
| `--vb`, `--vh` | Vellum (translucent layers) | Borders, hover states |
| `--sage` | Healthy / nominal | Status dots, "connected" indicators, healthy stone borders |
| `--clay` | Degraded / attention | Warnings, expiring credentials, replication lag |
| `--red` | Failed / unreachable | Errors, offline stones, failed syncs |
| `--gold` | Brand accent | Pavilion mark, ceremony-progress highlights, accent borders |

**Theming**: Pavilion uses the canonical dark palette from `tokens.css` rather than following Windows system theme. This keeps the visual identity uniform across the project (Lantern web + Pavilion desktop look the same). A future light-theme variant lives in `tokens.css`, not in Pavilion-local CSS.

**Window backdrop**: Solid `--bg`. **Mica/acrylic is NOT used** — translucent window backdrops fight translucent content (vellum layers) and degrade contrast against arbitrary desktop wallpapers.

**Color is augmented, not replaced.** Status states ALSO carry an icon and (for screen readers) text. Color-only state communication is forbidden.

### Typography

Two type families, defined as `--sans` and `--mono` in `tokens.css`:

- **`--sans`**: IBM Plex Sans — prose, labels, navigation, hero text
- **`--mono`**: IBM Plex Mono — URIs, identifiers, log lines, command output, file paths, status-bar text, tile labels (uppercase tracked)

System fallbacks (`system-ui`, `Cascadia Code`, `Consolas`) cover environments where IBM Plex isn't loaded. Bundled via `@fontsource/ibm-plex-sans` and `@fontsource/ibm-plex-mono` in the frontend.

Hierarchy uses size + weight + family, not color. Body text is the same weight everywhere; emphasis comes through the surrounding context (placement, size, mono-vs-sans).

### Motion

Motion is meaningful. The shared easing curve `--ease` (cubic-bezier(0.22, 1, 0.36, 1)) lives in `tokens.css`. Three classes:

1. **State transitions** (200–250ms, `--ease`): hover states, focus rings, panel slide-ins. Quick, purposeful.
2. **Status updates** (400–600ms): a stone goes offline; a sync completes; a service starts. Visible enough to register, not enough to interrupt.
3. **Ceremony progress** (1–2s, custom curves): pond enrollment, replant operations. The motion *narrates* what's happening.
4. **Health pip breath** (3s, infinite, `--ease`): the canonical `@keyframes breathe` animation defined in `tokens.css` — used on connected status dots and healthy stone borders. Idle pulse is permitted *only* on these health affordances; nothing else.

**No decorative animation.** No spinners on idle dashboards. No idle pulses on non-health items. Motion is reserved for change and for the breath of liveness.

### Iconography

Iconography is **functional, not branded**. Use Phosphor or Lucide (whichever the React package settles on) for the bulk. Reserve custom icons for zen-garden-specific concepts (Stone, Pond, Lantern, Bank, Companion) — and only when the standard libraries lack a clean equivalent.

Icons always pair with text labels in primary navigation. Icon-only buttons are reserved for secondary actions (in toolbars, context menus, tray) and always have an accessible label.

---

## 3. Layout

### Dashboard window

```
┌─ Pavilion ─────────────────────────────────────────────────────────┐
│ [garden ▾]   crystal-forest          [⌕ Ctrl+K]   [🔔 3]   [⚙]    │ ← top bar
├──────────┬─────────────────────────────────────────────────────────┤
│  Home    │                                                          │
│  Garden  │              ACTIVE PAGE CONTENT                         │
│  Storage │                                                          │
│  Services│                                                          │
│  Companions                                                         │
│  Pond    │                                                          │
│  Activity│                                                          │
│          │                                                          │
│  ──      │                                                          │
│  Settings│                                                          │
├──────────┴─────────────────────────────────────────────────────────┤
│ ● connected   ⟳ 12 syncing   pond healthy   quiet hours off       │ ← status bar
└────────────────────────────────────────────────────────────────────┘
```

- **Top bar**: garden switcher (left), breadcrumb, command palette trigger, notification bell, settings shortcut
- **Sidebar**: primary destinations (always visible)
- **Page content**: the active destination's view
- **Status bar**: persistent state summary

The dashboard window is **resizable**. Min size: 800×600. Default: 1280×800. Position remembered per monitor.

### Tray popover

Activated by left-click on the tray icon. Translucent (Win11 acrylic, Win10 plain). 360px wide, height adaptive (max 600px). Anchored to the tray icon.

```
┌─────────────────────────────┐
│ ● crystal-forest            │
│   3 stones · pond healthy   │
├─────────────────────────────┤
│ ⟳ Syncing                   │
│   12 files · 4.2 MB/s       │
├─────────────────────────────┤
│ Recent activity              │
│ • Service mongodb restarted │
│   2 min ago                 │
│ • New stone joined          │
│   8 min ago                 │
├─────────────────────────────┤
│ [Open Pavilion]   [Pause]   │
└─────────────────────────────┘
```

The popover is a glance, not a workspace. For anything beyond a quick check, "Open Pavilion" launches the full window.

---

## 4. Drag-and-drop vocabulary

The gesture is the primary affordance for direct manipulation. Every drag has a context-menu equivalent (keyboard-friendly), but drag is the *first-class* path.

### Drag sources

| Source | Represents | Examples |
|---|---|---|
| Stone card | A physical compute node | `crystal-forest` card on Garden page |
| Service card | A running offering | `mongodb` card on Services page |
| Bank card | A storage replica | `personal` bank on Storage page |
| File in browser | A file or folder | Files in the bank file browser |
| URI from external app | A `zen-garden:` deep link | Drag a URL from a chat |

### Drop targets

| Target | Semantic | Triggered action |
|---|---|---|
| Stone card | Place this thing on this stone | Replant service, plant offering, mirror bank |
| Stone group / set | Add to this collection | Add stone to pond, mirror to set |
| Bank card | Store / mirror | Upload file, add bank as replica |
| Empty space | Detach / split | Uproot service from stone, split bank from set |
| Pond | Enroll | Enroll a stone into the pond |
| Trash zone | Remove | Uproot, take away, delete |

### Drop semantics

| Drag → Drop | Action |
|---|---|
| Service → Stone | **Replant** ([ORCH-0001](../decisions/ORCH-0001-replant-ceremony.md)) |
| Offering catalog item → Stone | **Plant** |
| Stone → Pond | **Enroll** |
| Bank → Bank | **Mirror** (form a replica set) |
| Bank → empty space | **Split** (detach replica from set) |
| File → Bank | **Upload** |
| File → Stone (cascades to default bank) | **Upload to default bank** |
| Service → empty space | **Uproot** (with confirmation) |
| URI → window | **Navigate to deep link** |

### Drag affordances

- **Hover-and-hold for 400ms**: drop targets light up (`accent` border glow); invalid targets desaturate. This delay prevents accidental drops on accidental drags.
- **Drop preview**: a translucent ghost of the dragged item appears near the cursor; drop targets show a placement preview (where it would land, what would happen).
- **Cancel**: `Esc` cancels the drag at any time. Drop on no valid target also cancels (no-op).
- **Multi-select**: `Ctrl/Cmd-click` to add to selection; drag the selection en masse. Drops apply per-item.
- **Confirmation**: destructive drops (uproot, delete, drain) prompt. Constructive drops (replant, mirror) execute immediately with undo for 5s.

### Drag-and-drop is not the only path

Every drag-drop action has equivalents:

- **Right-click context menu** on the source object
- **Command palette** (`Ctrl+K`) action
- **Keyboard shortcuts** for the most common (replant: `Ctrl+R`)
- **CLI**: every action maps to a `garden-rake` command

This redundancy is intentional. Drag is for the eye-and-hand path; menus and keyboard for the keyboard path; CLI for scripting and accessibility.

---

## 5. Facilitators (proactive suggestions)

Facilitators are the *system's* contribution to the dialogue. Where drag-and-drop lets the user act on what they see, facilitators surface opportunities the user may not have noticed.

### Where they appear

Facilitators appear **inline** in the relevant view, not as global notifications. They are:

- **Banner-style** at the top of a page where the suggestion applies
- **Card-style** within a list, between existing items
- **Inline-link-style** in card details, where context warrants

They do **not**:
- Fire toasts (toasts are for events, not suggestions)
- Block the user (modals are for ceremonies, not pitches)
- Persist across sessions if dismissed (respect user's "no")

### Anatomy of a facilitator

```
┌─────────────────────────────────────────────────────────┐
│ 💡 Mirror these stones?                                 │
│                                                         │
│ crystal-forest and mossy-brook have similar GPUs and    │
│ the same offerings. Mirroring would give you fault     │
│ tolerance for ollama and mongodb.                      │
│                                                         │
│ [Mirror them]    [Not now]    [Hide this kind]          │
└─────────────────────────────────────────────────────────┘
```

Three controls always present:

- **Primary action** — single click executes the suggestion
- **Not now** — dismiss for this session; the suggestion may return
- **Hide this kind** — suppress this category of suggestion permanently (per setting)

### Suggestion sources

Facilitators are generated by *suggestion sources* — small components that observe garden state and emit candidate suggestions. The suggestion engine ranks candidates by relevance and surfaces at most one per page.

Initial sources (subject to refinement):

| Source | Triggers when | Suggestion |
|---|---|---|
| `mirror-similar-stones` | ≥2 stones share GPU class + offering set | "Mirror these stones?" |
| `replant-suboptimal` | Service runs on slower hardware than available | "Replant {service} to {stone}?" |
| `replicate-orphan-bank` | Bank has no replica | "Add a copy on {candidate}?" |
| `split-lagging-replica` | Replica is >24h behind primary | "Split this replica?" |
| `enroll-unclaimed-stone` | New stone discovered, not in any pond | "Enroll {stone} into your pond?" |
| `enable-pond` | Garden has 2+ stones, no pond | "Set up pond security?" |
| `expiring-cert` | Cert <30 days from expiry | "Renew enrollment?" |
| `unused-companion` | Companion installed, no events ever sent to it | "Configure {companion}?" |

### Suppression and quiet hours

Facilitators respect:

- **Quiet hours** (configurable in Settings) — no suggestions during these times
- **Per-source dismissal** — "Hide this kind" is permanent until reset in Settings
- **Per-suggestion cooldown** — the same specific suggestion does not return for 7 days after a "Not now"

### Facilitators are gentle, not pushy

The voice is tentative ("would," "could"), the framing acknowledges user autonomy, and the actions never run without explicit consent. A facilitator that the user dismisses is *not* a failure — it is the system respecting the user's decision.

---

## 6. Notifications (Windows toasts)

### When to fire a toast

Toasts are for *events that crossed a threshold*. Three categories:

1. **Identity events**: stone joined / left the garden; pond enrollment changed
2. **Operation outcomes**: sync completed / failed; service restarted / failed; backup completed
3. **Time-based warnings**: cert expiring; storage filling; update available

### When NOT to fire a toast

- Any event the user just initiated (the dashboard already shows it)
- Routine background activity (sync queue updates, polling results, cache refreshes)
- Suggestions (those are facilitators, not toasts)
- Errors the user hasn't asked about ("could not reach a stone you weren't using")

### Toast structure

- **Title**: the event in 1 line, < 50 chars
- **Body**: context in ≤ 3 lines, explaining *why* this matters
- **Actions** (where applicable): up to 2 buttons that do the next obvious thing

Example:

```
Title: "stone-amber-ridge offline"
Body:  "Lost contact 2 minutes ago. Services on it are unavailable."
Actions: [Check stone]  [Wake-on-LAN]
```

### Notification center

Every toast is *also* logged to Pavilion's in-app Activity view, with the action buttons still functional. This means a user who dismissed the toast (or had quiet hours on) can still revisit and act on it.

### Quiet hours

Toasts are suppressed during configured quiet hours. Critical events (stone offline, sync failed, cert expiring within 24h) still fire — the user can opt out of those individually in settings, but they are *opt-out*, not *opt-in*.

---

## 7. Modal flows (ceremonies)

Modals are reserved for **ceremonies** — multi-step flows where each step's outcome shapes the next. They are *not* for forms or settings.

### When a modal is the right shape

- Pond `init` / `join` / `invite` / `unlock` (security ceremonies)
- Replant (drag-initiated, but renders progress as modal)
- First-run onboarding
- Conflict resolution (Cloud Filter sync collision)

### When a modal is wrong

- Picking from a list of options (use a popover or inline picker)
- Editing a single field (use inline edit)
- Confirming a destructive action (use a small confirmation prompt, not a full modal)

### Modal anatomy

- **Title bar**: ceremony name + step indicator (e.g., "Pond enrollment · step 2 of 4")
- **Body**: the current step's prompt, input, or status
- **Actions**: forward (primary), back (secondary), cancel (tertiary or top-right close)
- **Progress visualisation**: animated where waiting on a remote operation; static where awaiting user input

Pond ceremonies render the QR code, TOTP entry, certificate fingerprint, and progress in this shape. The same state machine that drives Rake's terminal ceremony renderer ([src/rake/src/commands/ceremony_render.rs](../../src/rake/src/commands/ceremony_render.rs)) drives the modal — the model is shared, the surface differs.

---

## 8. Command palette (`Ctrl+K`)

The command palette is the keyboard-first entry point for everything Pavilion can do. It is the single highest-leverage UX investment.

### What it searches

- Stones, services, banks, companions (by name)
- Pinned URIs (saved by the user)
- All actions reachable from any view (replant, plant, restart, enroll, etc.)
- Pavilion settings
- Recent items (history)

### Behaviour

- Activated by `Ctrl+K` from anywhere in the dashboard
- Fuzzy search across the entire surface
- Selecting a result either *navigates* (to a view) or *executes* (an action)
- Actions that require parameters open a follow-up step inside the palette (no modal)
- `Esc` always cancels and closes

### Aliases and natural language

The palette accepts informal queries:

- "open mongo" → navigates to the mongodb service detail
- "restart redis on stone-01" → executes the action
- "where is file.txt" → searches across banks
- "mount personal" → opens the bank with mount options
- "zen-garden:..." (paste a URI) → resolves and navigates

Aliases are owned by `garden-rake`'s suggestion engine and reused; there is one source of truth for what natural-language inputs map to what actions.

---

## 9. Accessibility

Pavilion is built to **WCAG 2.2 AA** baseline.

- All interactive elements reachable by keyboard
- Focus order matches visual order
- Focus rings always visible (no `outline: none` without an equivalent)
- Color contrast: 4.5:1 for body, 3:1 for large text and UI components
- Screen-reader labels for every icon-only button
- Live regions announce status changes (sync started, stone offline)
- Drag-and-drop has a keyboard-equivalent path through context menus
- Toast notifications respect Windows reduce-motion settings

---

## 10. Open questions for the implementation pass

These are flagged here so they don't get lost; they are *not* blockers for starting M0.

- **Component library**: vanilla CSS + custom components, or a library (shadcn-style headless primitives, Radix UI)? Tauri-friendly lib survey needed.
- **Animation library**: Framer Motion, view transitions API, or hand-rolled?
- **Color tokens**: exact hex values per theme. Probably derived from Windows accent color where possible.
- **Tray popover technology**: Tauri 2's `WebviewWindow` with `decorated: false` and acrylic? Or native Windows popover via Tauri plugin?
- **Drag-and-drop library**: `dnd-kit` is the React standard; verify it works inside Tauri's WebView2.
- **Iconography pack**: Phosphor (more comprehensive) vs Lucide (smaller bundle).

---

## References

- [PAVILION-0001](../decisions/PAVILION-0001-windows-client-separation.md) — architectural foundation
- [URI-0003](../decisions/URI-0003-zen-garden-urn-form-scheme.md) — addressing scheme that maps to user vocabulary
- [DISC-0001](../decisions/DISC-0001-discovery-as-first-class-crate.md) — discovery primitive Pavilion consumes
- [ORCH-0001](../decisions/ORCH-0001-replant-ceremony.md) — replant flow surfaced via drag-drop
- [src/lantern/frontend/src/views/](../../src/lantern/frontend/src/views/) — existing React components Pavilion will reuse
- [src/rake/src/commands/ceremony_render.rs](../../src/rake/src/commands/ceremony_render.rs) — ceremony state machine shared with Pavilion modals
