//! Wire fixtures — R0.5 as tests. These pin the v0-compatible shape of v1
//! chirps and the envelope. If any of these fail, the fleet migration story
//! is broken; stop and fix the wire, not the test.

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
    let v = serde_json::to_value(&sample_body()).unwrap();
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
        serde_json::to_value(&sample_body()).unwrap(),
    );
    let wire = serde_json::to_string(&ann).unwrap();
    let back: Announcement = serde_json::from_str(&wire).unwrap();
    assert_eq!(back.kind, announcement::STONE_CHIRP);
    assert!(back.msg_id.is_some(), "v1 always carries msg_id for dedup");
    assert_eq!(back.data["stone_name"], "stone-proto");
}

#[test]
fn announcement_discriminators_match_the_poc_wire() {
    assert_eq!(announcement::STONE_CHIRP, "STONE_CHIRP");
    assert_eq!(announcement::STONE_GOODBYE, "STONE_GOODBYE");
    assert_eq!(announcement::DISCOVERY_REQUEST, "DISCOVERY_REQUEST");
    assert_eq!(announcement::ALL_V0.len(), 9);
}
