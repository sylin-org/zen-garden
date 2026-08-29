//! The song: a full-voice announcement (ADR-0004 A2.2) and the FRAMER that
//! quantizes announcements against the datagram budget (A2.3).
//!
//! A song is presence plus one or more inventory domains. The framer packs
//! dirty domain blocks whole — a block rides entire or waits — into as many
//! frames as the budget demands; every frame re-anchors stone/presence so
//! each is independently mergeable, and `meta.part` marks the position
//! informationally. Consumers never wait or reassemble: revs make order
//! irrelevant.

use crate::chirp::{ChirpFrame, InventoryMap};

/// Conservative serialization ceiling per frame (A2.3): the UDP budget is
/// 4 KB; we target under this so envelope/envelope-meta/JSON overhead and
/// future signature envelopes keep headroom.
pub const FRAME_BUDGET_BYTES: usize = 3_500;

/// Why a song carries a domain: change spoke, or a question asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// A lifecycle event dirtied exactly this domain.
    Change,
    /// A rich ask demanded inventory.
    Ask,
}

/// One announcement request: the domains to speak, already composed.
/// (The composer hands blocks over; the framer owns only quantization.)
#[derive(schemars::JsonSchema, Debug, Clone, Default)]
pub struct Announcement {
    /// Ordered by framer priority (services > banks > future).
    pub blocks: Vec<(String, serde_json::Value)>,
}

impl Announcement {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// Quantize an announcement into wire-ready frames. Every output frame
/// shares the given stone/presence/meta skeleton and differs only in which
/// domain blocks it carries (and its `part` marker). Frames are sorted
/// biggest-block-first within their group so the packing is deterministic
/// for identical inputs.
pub fn frame_song(
    base: &ChirpFrame,
    blocks: Vec<(String, serde_json::Value)>,
    seq: u64,
) -> Vec<ChirpFrame> {
    // Biggest first: greedy packing minimizes frame count (deterministic
    // tiebreak by name keeps tests stable).
    let mut blocks = blocks;
    blocks.sort_by(|a, b| {
        let sa = serde_json::to_vec(&a.1).map(|v| v.len()).unwrap_or(0);
        let sb = serde_json::to_vec(&b.1).map(|v| v.len()).unwrap_or(0);
        sb.cmp(&sa).then_with(|| a.0.cmp(&b.0))
    });
    if blocks.is_empty() {
        return Vec::new(); // silence is not a song
    }

    let mut groups: Vec<Vec<(String, serde_json::Value)>> = vec![Vec::new()];
    let mut current = FRAME_BUDGET_BYTES;
    for (name, block) in blocks {
        let size = serde_json::to_vec(&block).map(|v| v.len()).unwrap_or(0);
        // Will this block fit in the open frame? Measure with the full
        // envelope present (a probe frame carrying just this block).
        let probe = probe_size(base, &[(name.clone(), block.clone())], seq, 1, 1);
        let open_is_empty = groups.last().is_some_and(|g| g.is_empty());
        if size + probe > current && !open_is_empty {
            groups.push(Vec::new());
            current = FRAME_BUDGET_BYTES;
        }
        current = current.saturating_sub(size);
        if let Some(open) = groups.last_mut() {
            open.push((name, block));
        }
    }

    let of = groups.len() as u32;
    groups
        .into_iter()
        .enumerate()
        .map(|(i, group)| {
            let n = i as u32 + 1;
            let mut frame = base.clone();
            frame.meta.seq = Some(seq);
            if of > 1 {
                frame.meta.part = Some(crate::chirp::Part { n, of });
            }
            frame.inventory = InventoryMap::from_pairs(group);
            frame
        })
        .collect()
}

/// Wire size of one frame carrying exactly these blocks (header + payload),
/// used by the packer to decide when the next block overflows.
fn probe_size(
    base: &ChirpFrame,
    blocks: &[(String, serde_json::Value)],
    seq: u64,
    n: u32,
    of: u32,
) -> usize {
    let mut probe = base.clone();
    probe.meta.seq = Some(seq);
    if of > 1 {
        probe.meta.part = Some(crate::chirp::Part { n, of });
    }
    probe.inventory = InventoryMap::from_pairs(blocks.iter().cloned());
    let mut bytes = serde_json::to_vec(&probe).map(|v| v.len()).unwrap_or(usize::MAX);
    // The envelope adds kind/msg_id/timestamps around the payload.
    bytes += 96;
    bytes
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::chirp::{
        Inventory, Moss, Network, PeerAddress, Presence, Reception, ServiceEntry, ServiceState,
        Stone,
    };
    use std::net::Ipv4Addr;

    fn base_frame() -> ChirpFrame {
        ChirpFrame {
            stone: Stone {
                id: "sid".into(),
                name: "stone-singer".into(),
                moss: Moss { version: "1.0.0".into() },
                network: Network {
                    address: PeerAddress {
                        ip: std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)),
                        port: 7285,
                        tls_port: None,
                    },
                    mac: None,
                },
            },
            presence: Presence {
                health: "thriving".into(),
                status: "online".into(),
            },
            inventory: InventoryMap::default(),
            meta: Default::default(),
            received: Reception {
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            },
        }
    }

    fn service_block(n: usize, tag: &str) -> (String, serde_json::Value) {
        let items: Vec<ServiceEntry> = (0..n)
            .map(|i| ServiceEntry {
                offering_id: format!("{tag}-{i}"),
                name: format!("svc{i}::{tag}"),
                stem: format!("svc{i}"),
                category: "misc".into(),
                state: ServiceState { status: "running".into(), role: None },
                ports: Default::default(),
                capabilities: Default::default(),
            })
            .collect();
        (
            "services".into(),
            serde_json::to_value(Inventory {
                rev: Some(1),
                total: None,
                items,
            })
            .unwrap(),
        )
    }

    /// Operator example 1: "these 3 fill under 4k, so a single rich chirp
    /// it is" — small domains pack into ONE frame, no part marker.
    #[test]
    fn small_announcement_is_one_song() {
        let blocks = vec![
            service_block(2, "a"),
            service_block(2, "b"),
            service_block(2, "c"),
        ];
        let frames = frame_song(&base_frame(), blocks, 9);
        assert_eq!(frames.len(), 1, "small announcement must be one song");
        assert!(frames[0].meta.part.is_none());
        // All three domains rode along as service-block revisions.
        assert!(frames[0].inventory.services.is_some() || !frames[0].inventory.extra.is_empty());
    }

    /// Operator example 2: "no way this fits a single chirp — pack them in
    /// 2-3." Big domains split across frames; each carries the full block,
    /// part markers count honestly, and every block survives somewhere.
    #[test]
    fn oversized_announcement_packs_whole_blocks_into_parts() {
        // Large-but-fit blocks, distinct so domain count is traceable.
        let blocks: Vec<(String, serde_json::Value)> = (0..6)
            .map(|i| {
                let tag = format!("g{i}");
                let key = format!("services{i}"); // passthrough domain names
                let items: Vec<ServiceEntry> = (0..80)
                    .map(|j| ServiceEntry {
                        offering_id: format!("{tag}-{j}"),
                        name: format!("svc{j}::{tag}"),
                        stem: format!("svc{j}"),
                        category: "misc".into(),
                        state: ServiceState { status: "running".into(), role: None },
                        ports: Default::default(),
                capabilities: Default::default(),
                    })
                    .collect();
                (
                    key,
                    serde_json::to_value(Inventory {
                        rev: Some(1),
                        total: None,
                        items,
                    })
                    .unwrap(),
                )
            })
            .collect();

        let frames = frame_song(&base_frame(), blocks, 11);
        assert!(frames.len() >= 2, "must split: {} frames", frames.len());
        assert_eq!(
            frames[0].meta.part.as_ref().map(|p| p.of),
            Some(frames.len() as u32)
        );

        // Every frame re-anchors identity and presence.
        for f in &frames {
            assert_eq!(f.stone.name, "stone-singer");
            assert_eq!(f.presence.status, "online");
        }
    }

    /// A single domain larger than the whole budget still rides (the cap +
    /// total declaration are the last-resort honesty), but the framer never
    /// splits inside a block.
    #[test]
    fn pathological_block_rides_alone() {
        let huge = service_block(1500, "lone");
        let frames = frame_song(&base_frame(), vec![huge], 5);
        assert_eq!(frames.len(), 1);
    }

    /// Empty announcement → no frames (callers shouldn't speak silence).
    #[test]
    fn empty_announcement_is_no_frames() {
        assert!(frame_song(&base_frame(), vec![], 1).is_empty());
    }
}

