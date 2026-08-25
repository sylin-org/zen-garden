use crate::commands::offering::stone_prefer_score;

// NOTE: normalize_tokens, token_matches_category, and offering_relevance_score
// have been moved to Moss. Tests for those functions are in moss/src/api/v1/offerings.rs.
// Rake is now a thin client that calls Moss search API.
//
// The old Cli::parse_from / Commands / OfferAction tests were removed when the
// derive-based CLI was replaced by the manifest + builder architecture.

fn caps_with_disk_type(disk_type: &str) -> garden_common::HardwareCapabilities {
    garden_common::HardwareCapabilities {
        stone_id: None,
        stone_name: "stone-01".to_string(),
        hardware: garden_common::HardwareInventory {
            cpu: garden_common::CpuCapabilities {
                model: None,
                cores: 4,
                threads: None,
                architecture: "x86_64".to_string(),
                features: None,
            },
            memory: garden_common::MemoryCapabilities { total_mb: 8 * 1024 },
            gpus: vec![],
            disk: Some(garden_common::DiskCapabilities {
                total_gb: 128,
                disk_type: Some(disk_type.to_string()),
            }),
            swap_mb: None,
            ai_capabilities: None,
            system_manufacturer: None,
            system_product: None,
        },
        runtime: None,
        detection_status: garden_common::DetectionStatus::Complete,
    }
}

// NOTE: offering() helper removed - no longer needed since scoring tests moved to Moss

#[test]
fn stone_prefer_score_biases_nvme_and_ssd() {
    let caps = caps_with_disk_type("NVMe");

    assert_eq!(stone_prefer_score(&["ssd".to_string()], Some(&caps)), 10);
    assert_eq!(stone_prefer_score(&["nvme".to_string()], Some(&caps)), 12);
    assert_eq!(stone_prefer_score(&["hdd".to_string()], Some(&caps)), 0);
}

// Old tests `clap_parses_offer_prefer_and_anywhere_on_fail` and
// `clap_parses_offer_info_subcommand` removed — they tested the old derive-based
// Cli/Commands enum which was replaced by CommandManifest + builder API.
// The equivalent coverage is now in surface testing (--help pages, zen alias tests).
