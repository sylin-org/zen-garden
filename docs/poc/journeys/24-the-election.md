# The Election

*The founder goes offline. The garden elects new leadership.*

---

## The Story

Your garden has five Stones. stone-amber-ridge is the founder—it holds the Keystone and coordinates pond operations. It's been the leader since you created the pond two years ago.

Now you need to replace it. The hardware is aging. You bought a new machine. But you can't just unplug the founder—the garden needs someone to issue certificates, manage invitations, coordinate operations.

Time for an election.

---

### Preparing for Transition

First, you check the current state:

```bash
garden-rake pond roles
```

```
Pond Roles

  Keystone Holder: stone-amber-ridge (Founder)

  Role Distribution:
    stone-amber-ridge    Founder     Can: Everything
    stone-coral-reef     Admin       Can: Invite, Revoke, Deploy
    stone-bronze-canyon  Member      Can: Invite, Deploy
    stone-silver-creek   Member      Can: Invite, Deploy
    stone-golden-peak    Member      Can: Invite, Deploy

  Keystone Succession:
    1. stone-amber-ridge (current)
    2. stone-coral-reef (admin, next in line)
    3. (no other admins)

  Note: If the Keystone holder goes offline without transferring,
        stone-coral-reef will automatically assume leadership after 5 minutes.
```

stone-coral-reef is next in line. It's an admin—the only other Stone with elevated privileges. If stone-amber-ridge disappears, stone-coral-reef takes over.

---

### Orderly Transfer

You want an orderly transition, not an emergency election. You transfer the Keystone:

```bash
garden-rake pond transfer-keystone to stone-coral-reef
```

```
Transferring Keystone to stone-coral-reef...

  This will:
    • Transfer founder role to stone-coral-reef
    • Move Keystone authority to the new Stone
    • Demote stone-amber-ridge to Admin

  stone-amber-ridge will remain in the garden as an Admin.

  Enter current Keystone passphrase: ************

  Establishing secure channel to stone-coral-reef... done
  Transferring Keystone data... done
  Waiting for stone-coral-reef to confirm receipt... done

  New Keystone passphrase for stone-coral-reef.
  Generate automatically? [Y/n] y

  New passphrase (write this down):

      river-autumn-compass-31

  Confirm recorded by typing the last word: compass

  Activating new Keystone on stone-coral-reef... done
  Updating role: stone-amber-ridge → Admin... done
  Broadcasting leadership change... done

✓ Keystone transferred to stone-coral-reef

stone-coral-reef is now the Founder.
stone-amber-ridge is now an Admin.

You can safely retire stone-amber-ridge.
```

Leadership transferred. The old Stone is still in the garden but no longer holds the master keys.

---

### Verifying the Transfer

On any Stone:

```bash
garden-rake pond status
```

```
Pond Status

  State: ACTIVE
  Keystone: stone-coral-reef (Founder)
  Created: 2024-01-15T10:30:00Z
  Last Transfer: 2026-03-22T14:30:00Z

  Members (5):
    ● stone-coral-reef     Founder    Online    Certificate valid (58m)
    ● stone-amber-ridge    Admin      Online    Certificate valid (58m)
    ● stone-bronze-canyon  Member     Online    Certificate valid (58m)
    ● stone-silver-creek   Member     Online    Certificate valid (58m)
    ● stone-golden-peak    Member     Online    Certificate valid (58m)

  Security:
    ✓ All communications encrypted
    ✓ Leadership transfer completed
```

The garden knows stone-coral-reef is now in charge.

---

### Retiring the Old Stone

You can now safely remove stone-amber-ridge:

```bash
garden-rake pond leave
```

```
Leaving pond "home-garden"...

  This will:
    • Remove stone-amber-ridge from membership
    • Delete local credentials
    • You will need an invitation to rejoin

  You are currently an Admin. After leaving:
    • Founder: stone-coral-reef
    • Admins: (none remaining)

  Consider promoting another member to Admin before leaving.

Leave anyway? [y/N] y

  Notifying pond of departure... done
  Deleting local credentials... done
  Resetting to discovery mode... done

✓ You have left the pond

stone-amber-ridge is no longer a member of "home-garden".
```

The old Stone is out. You can wipe it, sell it, repurpose it—it has no garden secrets.

---

### The Emergency Election

A month later, something unexpected: stone-coral-reef's power supply fails. It goes offline suddenly, no graceful shutdown.

The remaining Stones notice:

```
[14:23:45] stone-coral-reef went offline (no goodbye, connection lost)
[14:23:45] Keystone holder offline - starting election timer
[14:28:45] Election timer expired (5 minutes)
[14:28:45] Initiating emergency leadership election
```

The Stones conduct an election:

```bash
# On any remaining Stone
garden-rake pond status
```

```
Pond Status

  State: ELECTION IN PROGRESS
  Previous Keystone: stone-coral-reef (OFFLINE - 5m 30s)

  Election Status:
    Eligible candidates: stone-bronze-canyon, stone-silver-creek, stone-golden-peak
    Votes cast: 3/3
    Leading: stone-bronze-canyon (2 votes)

  Waiting for election completion...
```

Seconds later:

```
Pond Status

  State: ACTIVE
  Keystone: stone-bronze-canyon (Emergency Founder)
  Previous: stone-coral-reef (offline, presumed failed)

  Members (4 active, 1 offline):
    ● stone-bronze-canyon  Emergency Founder  Online
    ● stone-silver-creek   Member            Online
    ● stone-golden-peak    Member            Online
    ○ stone-coral-reef     (offline, previous founder)

  Security:
    ✓ Communications encrypted
    ⚠ Emergency election completed
    ⚠ Previous Keystone passphrase unknown to new founder

  Note: stone-bronze-canyon holds temporary Keystone.
        When stone-coral-reef returns, manual reconciliation required.
```

The garden elected stone-bronze-canyon as emergency leader. The pond continues operating.

---

### When the Old Founder Returns

A week later, you replace stone-coral-reef's power supply. It comes back online:

```
Reconnecting to garden...

⚠ LEADERSHIP CONFLICT DETECTED

  You were the Founder, but an emergency election occurred.
  Current Founder: stone-bronze-canyon (emergency elected)

  Options:
    1. Accept new leadership (become Member)
    2. Reclaim leadership (requires passphrase proof)

  Contact your garden administrator to resolve.
```

On stone-bronze-canyon:

```
⚠ FORMER FOUNDER ONLINE

  stone-coral-reef has reconnected.
  It was the previous Founder before emergency election.

  Options:
    • Let it reclaim leadership: garden-rake pond restore-founder
    • Keep current leadership: garden-rake pond confirm-election
```

You decide to restore the original founder (it has the real Keystone passphrase):

```bash
garden-rake pond restore-founder stone-coral-reef
```

```
Restoring founder status to stone-coral-reef...

  stone-coral-reef will need to prove Keystone ownership.

  On stone-coral-reef, run:
    garden-rake pond prove-keystone

  Waiting for proof...
```

On stone-coral-reef:

```bash
garden-rake pond prove-keystone
```

```
Enter Keystone passphrase: ************

Proving Keystone ownership...
  Decrypting Keystone... done
  Generating proof challenge... done
  Sending proof to stone-bronze-canyon... done

✓ Proof accepted

Reclaiming founder status...
  Receiving temporary Keystone state... done
  Merging membership changes... done
  Resuming as Founder... done

✓ stone-coral-reef is now the Founder

Welcome back. The garden continued operating during your absence.
```

The original founder is back. The emergency leader gracefully steps down.

---

## What Just Happened

### The Leadership Hierarchy

The pond has a clear succession order:

```
┌─────────────────────────────────────────────────────────────────┐
│  LEADERSHIP HIERARCHY                                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  FOUNDER                                                        │
│  ├─ Holds the Keystone (master secret)                          │
│  ├─ Can do everything                                           │
│  ├─ Can transfer leadership                                     │
│  └─ Can drain the pond                                          │
│        │                                                        │
│        ▼ (succession)                                           │
│  ADMINS (ordered by join time)                                  │
│  ├─ Can invite/revoke members                                   │
│  ├─ Automatically become Founder if current one fails           │
│  └─ Cannot drain pond or transfer Keystone                      │
│        │                                                        │
│        ▼ (election if no admins)                                │
│  MEMBERS (elected by vote)                                      │
│  ├─ Can invite others                                           │
│  └─ Emergency Founder has limited powers                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Orderly Transfer Protocol

When you explicitly transfer the Keystone:

```
Old Founder                          New Founder
     │                                    │
     ├─ Verify passphrase                 │
     │  (prove ownership)                 │
     │                                    │
     ├─ Establish encrypted channel ─────►│
     │                                    │
     ├─ Transfer Keystone data ──────────►├─ Receive Keystone
     │   • Private key                    │
     │   • Membership database            │
     │   • Revocation list                │
     │                                    │
     │◄─────── Acknowledgment ───────────┤
     │                                    │
     │                                    ├─ Generate new passphrase
     │                                    │   (encrypts received Keystone)
     │                                    │
     ├─ Delete local Keystone             │
     │                                    │
     └─ Broadcast role change ───────────►└─ Assume Founder role
```

The new founder generates their own passphrase. The old passphrase is never transmitted—only the decrypted Keystone data travels over the secure channel.

### Emergency Election Protocol

When the Founder disappears unexpectedly:

```
Timeline:

t=0        Founder goes offline (no goodbye chirp)
           │
t=0-5m     Grace period
           • Maybe it's rebooting
           • Maybe network hiccup
           • Other Stones wait
           │
t=5m       Election timer expires
           │
           ├─ All online Stones become candidates
           │
           ├─ Each Stone votes for the candidate with:
           │   1. Highest role (Admin > Member)
           │   2. Longest uptime (tiebreaker)
           │   3. Lowest Stone ID (final tiebreaker)
           │
           ├─ Votes exchanged and tallied
           │
           └─ Winner becomes Emergency Founder
               • Can issue certificates
               • Can invite/revoke
               • CANNOT drain pond (no real Keystone)
               • CANNOT transfer leadership
```

Emergency Founders have limited powers. They keep the garden running but can't perform destructive operations like draining.

### The Proof of Keystone

When the original Founder returns, they can prove ownership:

```
Returning Founder                    Emergency Founder
       │                                    │
       ├─ Receive challenge nonce ◄─────────┤
       │                                    │
       ├─ Decrypt Keystone with passphrase  │
       │                                    │
       ├─ Sign challenge with private key   │
       │                                    │
       ├─ Send signed proof ────────────────►├─ Verify signature
       │                                    │   against known public key
       │                                    │
       │◄─────── Proof accepted ────────────┤
       │                                    │
       │                                    ├─ Transfer temp state:
       │◄── Membership changes ─────────────┤   • New members
       │    Issued certs                    │   • Revocations
       │    Activity logs                   │   • Cert history
       │                                    │
       ├─ Merge changes                     │
       │                                    │
       └─ Resume as Founder                 └─ Step down
```

The returning Founder learns about any changes that happened during their absence and merges them into their state.

### Split-Brain Prevention

What if the network partitions and both sides elect leaders?

```
Network Partition Scenario:

  Partition A                    Partition B
  ┌────────────────┐            ┌────────────────┐
  │ stone-coral    │            │ stone-silver   │
  │ (Founder)      │     X      │ stone-golden   │
  │ stone-bronze   │            │                │
  └────────────────┘            └────────────────┘
       2 Stones                      2 Stones
       Has Founder                   No Founder

  After 5 minutes:
  • Partition A: Continues normally (has Founder)
  • Partition B: Elects Emergency Founder

  When network heals:
  • Real Founder (stone-coral) takes precedence
  • Emergency Founder defers
  • Membership changes merged
```

The real Keystone always wins. Emergency elections are temporary measures that gracefully resolve when the real Founder returns.

### Why Not Consensus Algorithms?

Home gardens don't need Raft or Paxos:

| Feature | Consensus Algorithms | Zen Garden Election |
|---------|---------------------|---------------------|
| Consistency | Strong | Eventual |
| Availability | Requires quorum | Always available |
| Complexity | High | Low |
| Network requirement | Reliable | Best-effort |
| Failure mode | Halt without quorum | Continue degraded |

For home labs, availability matters more than strict consistency. A garden with two Stones should keep working even if one fails—waiting for quorum would defeat the purpose.

---

## Commands From This Journey

```bash
# View current roles and succession
garden-rake pond roles

# Transfer Keystone to another Stone
garden-rake pond transfer-keystone to stone-name

# Leave the pond voluntarily
garden-rake pond leave

# Promote a member to Admin
garden-rake pond promote stone-name to admin

# Demote an Admin to Member
garden-rake pond demote stone-name to member

# After emergency election, restore original Founder
garden-rake pond restore-founder stone-name

# Confirm emergency election (reject returning Founder)
garden-rake pond confirm-election

# Prove Keystone ownership (on returning Founder)
garden-rake pond prove-keystone

# Check election status during emergency
garden-rake pond election-status
```

---

*Zen Garden Documentation — Journeys*
