# Staying Focused

*Or: what Zen Garden is actually for.*

---

## The Mission

Zen Garden exists for three reasons:

1. **Reclaim e-waste** — That "obsolete" laptop, that retired office PC, that Raspberry Pi collecting dust. They still work. They should be useful.

2. **Restore ownership** — A small business owner shouldn't pay $500/month for managed Redis. A developer shouldn't need a cloud account to run Postgres. Your data, your network, your house.

3. **Remove barriers** — Self-hosting is hard because configuration is brittle. Zen Garden makes it easy: discover, deploy, connect.

---

## The User

The user is not a security professional. The user is:

- A developer who wants a local database without Docker Compose hell
- A small business owner who wants to stop paying Heroku prices
- A tinkerer who found three old ThinkPads and wants to do something useful
- A privacy advocate who wants their data on their own hardware
- Your aunt, who has a spare laptop and a nephew who set things up

The user wants to type `garden-rake offer mongodb` and have it work.

---

## The Adversary

The adversary is not the NSA. The adversary is:

| Threat | Likelihood | What Pond Does |
|--------|------------|----------------|
| Accidentally exposing MongoDB to the internet | HIGH | Pond = internal only |
| Neighbor's kid on your WiFi | MEDIUM | Encrypted traffic, admission control |
| Your own typos and mistakes | HIGH | Safety nets, rollback |
| Random port scanner on the internet | MEDIUM | Not listening externally |
| Nation-state APT | NEGLIGIBLE | Not addressed. Use different tools. |

If you need defense against nation-states, you need a security team, not a garden.

---

## What's Already Sufficient

The current Pond design provides:

| Protection | How |
|------------|-----|
| Encrypted traffic | XChaCha20-Poly1305 (same as WireGuard) |
| Admission control | TOTP 6-char codes, Bluetooth-pairing UX |
| Replay protection | Nonce tracking |
| Trust boundary | Inside pond = trusted, outside = outsider signal |

**This is enough.** Same cryptographic primitives that secure millions of WireGuard tunnels. Battle-tested. No novel cryptography.

The invitation flow:

```
garden-rake pond init --passphrase "my-pass" --profile just-me
garden-rake pond invite --passphrase "my-pass"
# Generate a 6-digit code from the TOTP URI. Done.
```

A small business owner can do this. A developer can do this. Your aunt can do this (with help).

---

## What We Don't Add

When evaluating new features, ask: *does a small business owner running MongoDB on old hardware need this?*

**Don't add:**

| Feature | Why Not |
|---------|---------|
| Multiple invitation modes | Cognitive load. One path is enough. |
| Authenticator app integration | Complexity for marginal benefit. |
| Enterprise compliance features | Those users have Kubernetes. |
| Certificate rotation | 30-day expiry with auto-renewal planned. |
| Multi-admin approval | Single trusted admin is the model. |
| MAC-based blocking | Doesn't work anyway. |

**The test:** If it requires explanation beyond one sentence, it's probably wrong for our users.

---

## When to Revisit

Add features when **real users ask for them**. Not:

- "What if someone wants..."
- "Enterprise customers might need..."
- "Security best practices say..."

Real users. Real requests. Real problems.

Until then, ship what works. The person reclaiming their old laptop doesn't care about your TOTP entropy calculations. They care that MongoDB starts and their app connects.

---

## The North Star

> A grandmother's retired laptop runs your development database. A small business stops paying cloud bills. An old PC becomes useful again. You own your compute.

Everything else is implementation detail.

---

**See also:**
- [Stone Against the Clouds](stone-against-the-clouds.md) — Why local ownership matters
- [Joy in Infrastructure](joy-in-infrastructure.md) — Making infrastructure delightful
- [Pond Security Model](pond-security-model.md) — How security works (when you need it)
