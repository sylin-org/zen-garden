use super::*;
use crate::commands::offering::stone_prefer_score;

// NOTE: normalize_tokens, token_matches_category, and offering_relevance_score
// have been moved to Moss. Tests for those functions are in moss/src/api/v1/offerings.rs.
// Rake is now a thin client that calls Moss search API.

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

#[test]
fn clap_parses_offer_prefer_and_anywhere_on_fail() {
    let cli = Cli::parse_from([
        "garden-rake",
        "offer",
        "database,document",
        "--prefer",
        "ssd,nvme",
        "--anywhere-on-fail",
    ]);

    match cli.command {
        Some(Commands::Offer {
            offering,
            prefer,
            anywhere_on_fail,
            ..
        }) => {
            assert_eq!(offering.as_deref(), Some("database,document"));
            assert_eq!(prefer, vec!["ssd".to_string(), "nvme".to_string()]);
            assert!(anywhere_on_fail);
        }
        _ => panic!("expected Commands::Offer"),
    }
}

#[test]
fn clap_parses_offer_info_subcommand() {
    let cli = Cli::parse_from(["garden-rake", "offer", "mongodb", "info"]);

    match cli.command {
        Some(Commands::Offer {
            offering, action, ..
        }) => {
            assert_eq!(offering.as_deref(), Some("mongodb"));
            assert!(matches!(action, Some(OfferAction::Info)));
        }
        _ => panic!("expected Commands::Offer"),
    }
}
