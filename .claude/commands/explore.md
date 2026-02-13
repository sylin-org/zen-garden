# Exploration Phase

Before implementing anything for this task, complete the following steps in order. Do not write any production code until all steps are done.

## Step 1: Understand the task

Restate the task in your own words. Identify:
- What concern does this touch? (API, domain, infrastructure, CLI, events, types)
- Which components are involved? (moss, rake, lantern, common)
- What's the expected output? (new feature, refactor, bug fix, extension)

## Step 2: Read existing code

Open and read the **3–5 most relevant existing files**. Use these searches to find them:

```bash
# Find types related to the task
rg "struct|enum|trait" src/ -l | head -20

# Find functions related to the task
rg "fn keyword_from_task" src/

# Find the closest existing implementation to what we're building
rg "similar_feature_keyword" src/ -l
```

For each file you read, state in one sentence what it does and whether it's relevant.

## Step 3: Check common/ for reusable code

Run these searches explicitly and report the results:

```bash
# Types that might already exist
rg "struct|enum" src/common/src/types/ 
rg "struct|enum" src/common/src/events/
rg "struct|enum" src/common/src/presence/

# Helpers that might already exist
rg "pub fn" src/common/src/utils.rs
rg "pub fn" src/common/src/constants/

# Constants that might already exist
rg "pub const" src/common/src/constants/
rg "pub const" src/common/src/presence/event_types.rs
```

State clearly: **"Found in common"** or **"Not in common, needs to be created"** for each piece of functionality the task requires.

## Step 4: Identify the closest pattern to follow

Find the most similar existing feature in the codebase. For example:
- New API endpoint → read an existing endpoint in `moss/src/api/v1/`
- New CLI command → read an existing command in `rake/src/commands/`
- New event type → read `common/src/events/domain_events.rs`
- New background task → read an existing task in `moss/src/tasks/`
- New shared type → read `common/src/types.rs` or relevant submodule

State: **"Following the pattern from [specific file]"**

## Step 5: Plan where new code will live

For every new file, type, function, or constant, state its location and justify it:

| New code | Location | Justification |
|----------|----------|---------------|
| (type/fn/const) | (exact path) | (why here and not elsewhere) |

Apply the common-first decision tree:
- Used by 2+ components? → `common/`
- Could be used by 2+ components? → `common/`
- Is it a contract (trait, type, event)? → `common/`
- Is it infrastructure (network, IO, parse)? → `common/`
- Is it component-specific domain logic? → that component's `domain/`

## Step 6: Check for potential violations

Before proceeding, confirm:

- [ ] No new type duplicates something in `common/src/types/` or `common/src/events/`
- [ ] No new string literal should be a constant in `common/src/constants/` or `common/src/presence/event_types.rs`
- [ ] No new function duplicates something in `common/src/utils.rs` or another common module
- [ ] No infrastructure code is planned for a `domain/` directory
- [ ] No domain code is planned for `common/` (common holds infrastructure and contracts, not business logic)

## Step 7: Present the plan

Summarize your findings in this format:

**Task:** (one sentence)
**Files read:** (list with one-sentence relevance notes)
**Reusing from common:** (list what already exists)
**Creating new:** (table from Step 5)
**Pattern:** (which existing file you're following)
**Risks:** (anything you're unsure about)

**Then stop and wait for approval before implementing.**
