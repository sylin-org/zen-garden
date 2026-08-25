---
audience: developer
doc_type: decision
status: accepted
---

# ORCH-0032: Unified Sidebar + Breadcrumb-as-Navigation

**Date**: 2026-04-10
**Status**: Accepted
**Deciders**: Leo
**Amends**: ORCH-0031 (dashboard architecture) — replaces the
three-surface tab sidebar and per-surface sub-sidebars with a single
fixed sidebar and a breadcrumb header that doubles as lateral navigation.

---

## Context

ORCH-0031 specified a sidebar with three surface tabs (Create, Manage,
Configure) where each surface owned its own sidebar content — the Create
surface embedded a full tool tree, Manage and Configure had tab bars.
During implementation this produced a double-sidebar layout: the Shell
rendered a 220px sidebar (logo + tabs + footer), and the Create surface
added a second 220px sidebar for the tool tree.

Two sidebars is a layout artifact, not a user need. The user needs to
see where they are, what's available, and how to get there — all in one
place, all the time.

---

## Decision

### 1. Single fixed sidebar

The sidebar is **always the same shape** regardless of which page the
user is on. It shows three groups at the top level, each with a short
list of leaves:

```
Zen Garden AI
─────────────────
CREATE
  💬 Text
  🖼️ Image
  🔊 Audio

MANAGE
  Skills
  Jobs
  Media

CONFIGURE
  Preferences
  Garden
  Providers
  Events

[● Connected  7 providers]
```

- **Group headers** (CREATE, MANAGE, CONFIGURE) are clickable → navigate
  to the group's directory/overview page.
- **Leaves** are clickable → navigate to the default sub-function of
  that leaf.
- The sidebar never collapses, expands, or changes content. It is a
  permanent, shallow table of contents.
- The active leaf is highlighted. The active group header is highlighted
  when its directory page is shown.

### 2. Leaf defaults

Each leaf maps to one default primitive or page:

| Sidebar leaf | Default route | Why |
|-------------|---------------|-----|
| Text | `/create/text/chat` | Chat is the primary text capability |
| Image | `/create/image/generate` | Generation is the primary image capability |
| Audio | `/create/audio/generate` | TTS is the primary audio capability |
| Skills | `/manage/skills` | Direct |
| Jobs | `/manage/jobs` | Direct |
| Media | `/manage/media` | Direct |
| Preferences | `/configure/preferences` | Direct |
| Garden | `/configure/garden` | Direct |
| Providers | `/configure/providers` | Direct |
| Events | `/configure/events` | Direct |

### 3. Breadcrumb header with sibling navigation

The header bar above the main area shows a breadcrumb that is also
lateral navigation. The active segment is highlighted; sibling segments
at the same depth are shown inline as clickable links.

**Primitive level** (`/create/image/generate`):

```
Create › Image › [Generate]  Edit  Upscale  Analyze
```

"Generate" is the active leaf. "Edit", "Upscale", "Analyze" are its
siblings under the same modality — shown right next to the breadcrumb
as clickable links. The user sees where they are and what else is
available in the same line.

**Skill level** (`/create/image/generate/animij-36771`):

```
Create › Image › Generate › [Animij 36771]
```

At the skill level, sibling skills are not shown in the breadcrumb
(there may be 14+). The skill picker in the main area handles lateral
skill navigation.

**Manage/Configure** (`/manage/skills`):

```
Manage › [Skills]
```

Simple — no siblings at this depth (the siblings are in the sidebar).

### 4. Breadcrumb segments are clickable upward

| Click on | Navigates to |
|----------|-------------|
| "Create" | `/create` (directory overview) |
| "Image" | `/create/image/generate` (default leaf for Image) |
| "Generate" | `/create/image/generate` (already there — no-op) |
| "Manage" | `/manage` (overview) |

### 5. Data-driven

The breadcrumb content comes from the backend:

- Group labels: hardcoded (CREATE, MANAGE, CONFIGURE — structural, not
  data)
- Modality labels and icons: from `GET /v1/catalog` → `modalities[]`
- Primitive siblings: from `GET /v1/catalog` → `primitives[]` filtered
  by the current modality
- Skill display name: from `GET /v1/catalog/{mod}/{leaf}/{skill}` →
  `display_name`

No client-side label maps. The breadcrumb renders what the catalog
provides.

### 6. Depth lives in the main area

The sidebar is only two levels deep (group → leaf). Everything deeper
— primitives within a modality, skills within a primitive, settings
sections — lives in the main content area. This keeps the sidebar
permanently stable while the main area handles the depth.

---

## Consequences

### Positive

- **Single sidebar, always visible.** No double-sidebar layout, no
  accordion state, no collapse toggling.
- **Breadcrumb is navigation.** No wasted header space — the
  breadcrumb that shows where you are also lets you move laterally.
- **Spatial proximity.** Sibling navigation is next to the active
  item, not in a separate tab bar or sidebar section.
- **Scales naturally.** Text has 3 siblings, Image has 4, Audio has 2.
  All rendered identically by the same breadcrumb component.

### Negative

- **Sidebar is shallow.** Users who want to jump directly to a skill
  from the sidebar can't — they go to the modality first, then pick
  from the main area. Acceptable: the sidebar is for orientation, the
  main area is for work.

---

## References

- [ORCH-0031](ORCH-0031-dashboard-architecture.md) — parent ADR
