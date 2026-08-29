//! Wire fixtures — R0.5 as tests, pinned to the CANONICAL frame (ADR-0004
//! amendment: records are paths). The v0-compat story is RETIRED — v1 owns
//! its room (own group/port/namespace; PoC fleet frozen at `poc-final`);
//! these fixtures now guard the one shape spoken across wire, cache, HTTP,
//! and rake. If these fail, the canonical-shape contract is broken; stop
//! and fix the wire, not the test.

// R4.1: unwrap_used is denied in domain code but sanctioned in tests.
#![allow(clippy::unwrap_used)]

use garden_contract::chirp::{
    ChirpFrame, Inventory, InventoryMap, Moss, Network, PeerAddress, Presence, Reception,
    ServiceEntry, ServiceState, Stone, INVENTORY_CAP,
};
use garden_contract::consts::{self, announcement};
use garden_contract::wire::Announcement;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};

/// The canonical frame, composed section by section (construction-site
/// ergonomics: Default + struct-update — the flat-field-zoo era is gone).
fn sample_frame() -> ChirpFrame {
    ChirpFrame {
        stone: Stone {
            id: "0198e0c7-0000-7000-8000-000000000001".into(),
            name: "stone-proto".into(),
            moss: Moss { version: "1.0.0-alpha".into() },
            network: Network {
                address: PeerAddress {
                    ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 137)),
                    port: 7285,
                    tls_port: None,
                },
                mac: None,
            },
        },
        presence: Presence {
            health: garden_glossary::health::THRIVING.into(),
            status: garden_glossary::presence::ONLINE.into(),
        },
        inventory: garden_contract::chirp::InventoryMap {
            services: Some(Inventory {
                rev: Some(7),
                total: None,
                items: vec![ServiceEntry {
                    offering_id: "0198e0c7-0000-7000-8000-0000000000aa".into(),
                    name: "memcached::default".into(),
                    stem: "memcached".into(),
                    category: "cache".into(),
                    state: ServiceState { status: "running".into(), role: None },
                    ports: Default::default(),
                capabilities: Default::default(),
                }],
            }),
            ..Default::default()
        },
        meta: garden_contract::chirp::FrameMeta {
            proto: Some(consts::PROTO_V1.into()),
            boot_id: Some("0198e0c7-0000-7000-8000-0000000000bb".into()),
            seq: Some(42),
            part: None,
        },
        received: Reception {
            discovered_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
        },
    }
}

/// Section spelling on the wire: rootspace holds sections, sections hold
/// facts, and every nesting level is a nameable noun. The inventory map's
/// known domain appears nested; unknown domains would join it as siblings.
#[test]
fn canonical_sections_are_present_on_the_wire() {
    let v = serde_json::to_value(sample_frame()).unwrap();
    for section in ["stone", "presence", "inventory", "meta", "received"] {
        assert!(v.get(section).is_some(), "canonical section `{section}` missing");
    }
    // Paths, not underscores: identity nests; the frame speaks FQNs.
    assert_eq!(v["stone"]["id"], "0198e0c7-0000-7000-8000-000000000001");
    assert_eq!(v["stone"]["network"]["address"]["port"], 7285);
    assert_eq!(v["stone"]["moss"]["version"], "1.0.0-alpha");
    assert_eq!(v["inventory"]["services"]["items"][0]["name"], "memcached::default");
    assert_eq!(v["inventory"]["services"]["items"][0]["stem"], "memcached");
    assert_eq!(v["inventory"]["services"]["rev"], 7);
}

/// Optional-noise discipline: None options must not emit; the frame stays
/// as small as its facts.
#[test]
fn absent_options_do_not_emit() {
    let v = serde_json::to_value(sample_frame()).unwrap();
    assert!(v.get("tls_port").is_none());
    assert!(v["stone"]["network"].get("mac").is_none());
    assert!(v["inventory"]["services"].get("total").is_none(), "undeclared total = no truncation");
    assert!(v["inventory"]["services"]["items"][0]["state"].get("role").is_none());
}

/// Envelope discipline: same discriminator, msg_id for dedup, sectioned
/// payload inside.
#[test]
fn envelope_roundtrip_preserves_discriminator() {
    let ann = Announcement::new(
        announcement::STONE_CHIRP,
        serde_json::to_value(sample_frame()).unwrap(),
    );
    let wire = serde_json::to_string(&ann).unwrap();
    let back: Announcement = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.kind, announcement::STONE_CHIRP);
    assert!(back.msg_id.is_some(), "v1 always carries msg_id for dedup");
    assert_eq!(back.data["stone"]["name"], "stone-proto");
}

/// Unknown future fields are ignored — forward compatibility is tolerance,
/// not shape-locking.
#[test]
fn unknown_future_fields_are_ignored() {
    let mut v = serde_json::to_value(sample_frame()).unwrap();
    v["future_thing"] = json!({ "watts": 5 });
    let frame: ChirpFrame = serde_json::from_value(v).unwrap();
    assert_eq!(frame.meta.seq, Some(42));
}

/// A frame that merely ANSWERED an ask carries the starting-health hint
/// (W1 precedent) and no inventory of its own.
#[test]
fn answered_frames_hint_health_and_carry_no_inventory() {
    let f = ChirpFrame::answered(
        "stone-echo",
        PeerAddress {
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9)),
            port: 7285,
            tls_port: None,
        },
        "1.0.0",
    );
    assert_eq!(f.presence.health, "starting");
    assert!(f.inventory.services.as_ref().is_none_or(|s| s.items.is_empty()));
    assert!(f.inventory.services.as_ref().and_then(|s| s.rev).is_none());
}

/// R0.5 pin: discriminators are LOWERCASE on the wire — transcribed from
/// poc announcement_types.rs. Capitalized variants would be silently
/// ignored by every garden speaker.
#[test]
fn announcement_discriminators_are_lowercase() {
    assert_eq!(announcement::STONE_CHIRP, "stone_chirp");
    assert_eq!(announcement::STONE_GOODBYE, "stone_goodbye");
    assert_eq!(announcement::DISCOVERY_REQUEST, "discovery_request");
    assert_eq!(announcement::DISCOVERY_RESPONSE, "discovery_response");
}

/// The ask/tell grammar: rich asks flag themselves; lean asks stay silent
/// on the wire; rich answers carry the same inventory block as the frame.
#[test]
fn discovery_grammar_rich_and_lean() {
    let lean = garden_contract::discovery::DiscoveryRequest::for_moss("rake");
    let v = serde_json::to_value(&lean).unwrap();
    assert!(v.get("rich").is_none(), "lean must not emit the flag at all");

    let rich = garden_contract::discovery::DiscoveryRequest::for_moss_rich("newcomer");
    let v = serde_json::to_value(&rich).unwrap();
    assert_eq!(v["rich"], true);

    let res = garden_contract::discovery::DiscoveryResponse {
        stone: Stone {
            id: "sid".into(),
            name: "stone-rich".into(),
            moss: Moss { version: "1.0.0".into() },
            network: Network {
                address: PeerAddress {
                    ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9)),
                    port: 7285,
                    tls_port: None,
                },
                mac: None,
            },
        },
        lantern_endpoint: None,
        inventory: InventoryMap {
            services: Some(Inventory { rev: Some(3), total: Some(24), items: vec![] }),
            ..Default::default()
        },
    };
    let v = serde_json::to_value(&res).unwrap();
    assert_eq!(v["stone"]["name"], "stone-rich");
    assert_eq!(v["inventory"]["services"]["rev"], 3);
    assert_eq!(v["inventory"]["services"]["total"], 24);
}

/// The wire cap is part of the protocol — pinned so nobody "improves" it
/// without noticing envelopes grow past budget.
#[test]
fn inventory_cap_is_the_advertised_constant() {
    assert_eq!(INVENTORY_CAP, 24);
}

/// R1.7/R0.5 pin: the typed multicast groups equal their historical dotted
/// forms — one truth per room, the other pinned.
#[test]
fn multicast_group_consts_match_historical_dotted_forms() {
    assert_eq!(consts::MULTICAST_GROUP_V1.to_string(), consts::MULTICAST_GROUP_V1_STR);
    assert_eq!(
        consts::MULTICAST_GROUP_POC.to_string(),
        consts::MULTICAST_GROUP_POC_STR
    );
}

#[test]
fn banks_merge_by_revision_like_services() {
    use garden_contract::chirp::{BankEntry, Inventory, InventoryMap};

    let bank = |state: &str| BankEntry {
        fqn: "seed-vault::default".into(),
        device_id: "dev-1".into(),
        state: state.into(),
        roles: vec![],
        capacity_bytes: None,
        used_bytes: None,
    };
    let block = |rev: u64, state: &str| Inventory {
        rev: Some(rev),
        total: None,
        items: vec![bank(state)],
    };

    let mut cache = InventoryMap {
        banks: Some(block(2, "mounted")),
        ..Default::default()
    };
    let stale = InventoryMap {
        banks: Some(block(1, "ejected")),
        ..Default::default()
    };
    cache.merge_frame(&stale);
    assert_eq!(
        cache.banks.as_ref().unwrap().items[0].state,
        "mounted",
        "a stale rev never beats a fresh one"
    );

    let fresh = InventoryMap {
        banks: Some(block(3, "ejected")),
        ..Default::default()
    };
    cache.merge_frame(&fresh);
    assert_eq!(
        cache.banks.as_ref().unwrap().items[0].state,
        "ejected",
        "the newer rev speaks (ADR-0005 sec 8.3)"
    );
}
