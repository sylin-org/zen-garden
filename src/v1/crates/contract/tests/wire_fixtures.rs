//! Wire fixtures — R0.5 as tests. These pin the v0-compatible shape of v1
//! chirps and the envelope. If any of these fail, the fleet migration story
//! is broken; stop and fix the wire, not the test.

// R4.1: unwrap_used is denied in domain code but sanctioned in tests.
#![allow(clippy::unwrap_used)]

use garden_contract::chirp::{ChirpBody, PeerAddress, ServiceEntry};
use garden_contract::consts::{self, announcement};
use garden_contract::wire::Announcement;
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr};

fn sample_body() -> ChirpBody {
    ChirpBody {
        stone_id: "0198e0c7-0000-7000-8000-000000000001".into(),
        stone_name: "stone-proto".into(),
        address: PeerAddress {
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 137)),
            port: 7285,
            tls_port: None,
        },
        moss_version: "1.0.0-alpha".into(),
        services: vec![ServiceEntry {
            offering_id: "0198e0c7-0000-7000-8000-0000000000aa".into(),
            name: "mongodb".into(),
            offering: "mongodb".into(),
            category: "data".into(),
            status: "running".into(),
            role: None,
        }],
        health: garden_glossary::health::THRIVING.into(),
        status: garden_glossary::presence::ONLINE.into(),
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
        mac: None,
        proto: Some(consts::PROTO_V1.into()),
        boot_id: Some("0198e0c7-0000-7000-8000-0000000000bb".into()),
        seq: Some(42),
    }
}

#[test]
fn v0_required_keys_are_present_on_the_wire() {
    let body = sample_body();
    let v = serde_json::to_value(&body).unwrap();
    for key in [
        "stone_id",
        "stone_name",
        "address",
        "moss_version",
        "services",
        "health",
        "status",
        "discovered_at",
        "last_seen",
    ] {
        assert!(v.get(key).is_some(), "v0-required key `{key}` missing");
    }
    let addr = v.get("address").unwrap();
    assert!(addr.get("ip").is_some() && addr.get("port").is_some());
}

#[test]
fn v0_optional_keys_are_absent_when_none() {
    let v = serde_json::to_value(sample_body()).unwrap();
    assert!(v.get("tls_port").is_none(), "None options must not emit");
    assert!(v.get("mac").is_none());
}

#[test]
fn parses_a_v0_shaped_chirp() {
    // Shape mirrors the PoC's chirp: v0-required core, no v1 extensions,
    // plus a capabilities field v1 does not model — unknown to us, ignored.
    let v0 = json!({
        "stone_id": "8f94010a-1071-52ba-b223-702eeedb0501",
        "stone_name": "stone-emerald-vale",
        "address": { "ip": "192.168.1.82", "port": 7185 },
        "moss_version": "0.2.0.202606101315",
        "services": [ { "name": "mongodb", "offering": "mongodb",
                        "category": "data", "status": "running" } ],
        "health": "thriving",
        "status": "online",
        "discovered_at": "2026-08-25T01:09:28.648917114Z",
        "last_seen": "2026-08-25T01:09:28.648917114Z",
        "capabilities": null,
        "tags": []
    });
    let body: ChirpBody = serde_json::from_value(v0).unwrap();
    assert_eq!(body.stone_name, "stone-emerald-vale");
    assert_eq!(body.services.len(), 1);
    assert!(body.proto.is_none(), "v0 chirps carry no proto marker");
}

#[test]
fn unknown_future_fields_are_ignored() {
    let mut body = sample_body();
    let mut v = serde_json::to_value(&body).unwrap();
    v["future_thing"] = json!({ "watts": 5 });
    body = serde_json::from_value(v).unwrap();
    assert_eq!(body.seq, Some(42));
}

#[test]
fn envelope_roundtrip_preserves_discriminator() {
    let ann = Announcement::new(
        announcement::STONE_CHIRP,
        serde_json::to_value(sample_body()).unwrap(),
    );
    let wire = serde_json::to_string(&ann).unwrap();
    let back: Announcement = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.kind, announcement::STONE_CHIRP);
    assert!(back.msg_id.is_some(), "v1 always carries msg_id for dedup");
    assert_eq!(back.data["stone_name"], "stone-proto");
}

/// R0.5 pin: discriminators are LOWERCASE on the v0 wire — transcribed
/// from poc/common/src/infra/communications/announcement_types.rs. A
/// capitalized variant would be silently ignored by every PoC stone.
#[test]
fn announcement_discriminators_match_the_poc_wire() {
    assert_eq!(announcement::STONE_CHIRP, "stone_chirp");
    assert_eq!(announcement::STONE_GOODBYE, "stone_goodbye");
    assert_eq!(announcement::DISCOVERY_REQUEST, "discovery_request");
    assert_eq!(announcement::DISCOVERY_RESPONSE, "discovery_response");
    assert_eq!(announcement::ELECTION_REQUEST, "election_request");
    assert_eq!(announcement::ELECTION_CANDIDATE, "election_candidate");
    assert_eq!(announcement::ELECTION_RESULT, "election_result");
    assert_eq!(announcement::STORAGE_BEACON, "storage_beacon");
    assert_eq!(announcement::TOOLS_BEACON, "tools_beacon");
    assert_eq!(announcement::ALL_V0.len(), 9);
}

/// R0.5 pin: the ask/tell shapes, transcribed from
/// poc/common/src/types/discovery.rs.
#[test]
fn discovery_request_shape_matches_v0() {
    let req = garden_contract::discovery::DiscoveryRequest {
        discover: "moss".into(),
        request_id: "0198e0c7-0000-7000-8000-0000000000f1".into(),
        requester: "rake-cli".into(),
    };
    let v = serde_json::to_value(&req).unwrap();
    assert_eq!(
        v,
        json!({
            "discover": "moss",
            "request_id": "0198e0c7-0000-7000-8000-0000000000f1",
            "requester": "rake-cli",
        })
    );
}

#[test]
fn discovery_response_omits_absent_options() {
    use std::net::IpAddr;
    let res = garden_contract::discovery::DiscoveryResponse {
        stone_id: Some("sid".into()),
        stone_name: "stone-x".into(),
        address: garden_contract::chirp::PeerAddress {
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9)),
            port: 7285,
            tls_port: None,
        },
        moss_version: "1.0.0".into(),
        lantern_endpoint: None,
    };
    let v = serde_json::to_value(&res).unwrap();
    assert!(v.get("lantern_endpoint").is_none());
    assert_eq!(v["stone_name"], "stone-x");
    // stone_id present when Some, absent when None
    assert_eq!(v["stone_id"], "sid");
    let bare = garden_contract::discovery::DiscoveryResponse {
        stone_id: None,
        ..res
    };
    let v = serde_json::to_value(&bare).unwrap();
    assert!(v.get("stone_id").is_none());
}

/// R1.7/R0.5 pin: the typed multicast groups equal their historical dotted
/// forms — one truth per room, the other pinned. The PoC group is legacy
/// reference only; the v1 group is the default room.
#[test]
fn multicast_group_consts_match_historical_dotted_forms() {
    assert_eq!(consts::MULTICAST_GROUP_V1.to_string(), consts::MULTICAST_GROUP_V1_STR);
    assert_eq!(
        consts::MULTICAST_GROUP_POC.to_string(),
        consts::MULTICAST_GROUP_POC_STR
    );
}
