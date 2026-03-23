//! Name generation for garden entities
//!
//! Provides themed dictionaries and generators for naming garden entities.
//! Each entity type has a distinct thematic vocabulary:
//!
//! - **Stones** (Linux): landscape & natural formations (see `console::tty`)
//! - **Stones** (Windows): stained glass & sacred architecture (see `console::tty`)
//! - **Seed banks**: botanical & harvest (see `api::v1::storage`)
//! - **Ponds**: water, reflection, stillness — the theme of this module
//!
//! All names follow the pattern `{entity}-{adjective}-{noun}`.

use rand::prelude::IndexedRandom;
use rand::SeedableRng;

// ═══════════════════════════════════════════════════════════════════════
// Pond naming — water, reflection, depth, stillness, aquatic life
// ═══════════════════════════════════════════════════════════════════════

/// 64 adjectives — water qualities, light on water, depth, stillness
const POND_ADJECTIVES: &[&str] = &[
    // Surface & movement (16)
    "still",
    "rippled",
    "glassy",
    "drifting",
    "flowing",
    "swirling",
    "eddying",
    "mirrored",
    "wavering",
    "lapping",
    "buoyant",
    "floating",
    "wading",
    "brimming",
    "cresting",
    "surging",
    // Depth & clarity (16)
    "deep",
    "shallow",
    "clear",
    "murky",
    "pellucid",
    "fathomless",
    "sunken",
    "steeped",
    "submerged",
    "abyssal",
    "limpid",
    "lucent",
    "vitreous",
    "briny",
    "brackish",
    "silted",
    // Light on water (16)
    "gleaming",
    "shimmering",
    "glistening",
    "sparkling",
    "luminous",
    "opalescent",
    "iridescent",
    "pearlescent",
    "phosphorescent",
    "moonlit",
    "sunlit",
    "dappled",
    "twinkling",
    "glinting",
    "lustrous",
    "radiant",
    // Temperature & atmosphere (16)
    "cool",
    "warm",
    "frozen",
    "thawing",
    "steaming",
    "misty",
    "dewy",
    "humid",
    "balmy",
    "brisk",
    "tepid",
    "glacial",
    "temperate",
    "vernal",
    "autumnal",
    "wintry",
];

/// 64 nouns — water bodies, aquatic life, water phenomena
const POND_NOUNS: &[&str] = &[
    // Water bodies (16)
    "basin",
    "cistern",
    "wellspring",
    "reservoir",
    "estuary",
    "lagoon",
    "inlet",
    "channel",
    "tributary",
    "confluence",
    "watershed",
    "aquifer",
    "millpond",
    "oxbow",
    "billabong",
    "cenote",
    // Water features (16)
    "cascade",
    "waterfall",
    "fountain",
    "geyser",
    "rapids",
    "whirlpool",
    "maelstrom",
    "tideline",
    "current",
    "undertow",
    "spillway",
    "weir",
    "sluice",
    "flume",
    "aqueduct",
    "runnel",
    // Aquatic & riparian (16)
    "lily",
    "lotus",
    "reed",
    "willow",
    "cattail",
    "duckweed",
    "sedge",
    "fern",
    "heron",
    "kingfisher",
    "otter",
    "newt",
    "tadpole",
    "dragonfly",
    "mayfly",
    "damselfly",
    // Reflections & phenomena (16)
    "reflection",
    "ripple",
    "shimmer",
    "droplet",
    "mist",
    "dewdrop",
    "raindrop",
    "puddle",
    "eddy",
    "wake",
    "swell",
    "foam",
    "spray",
    "torrent",
    "deluge",
    "freshet",
];

/// Generate a random pond name: `pond-{adjective}-{noun}`
///
/// 64 adjectives × 64 nouns = 4,096 unique combinations.
/// No collision detection needed — pond names are decorative identifiers
/// that the user can change at any time without consequences.
pub fn generate_pond_name() -> String {
    let mut rng = rand::rngs::StdRng::from_os_rng();
    let adjective = POND_ADJECTIVES.choose(&mut rng).unwrap();
    let noun = POND_NOUNS.choose(&mut rng).unwrap();
    format!("pond-{}-{}", adjective, noun)
}

/// Validate a pond name format.
///
/// Must match `pond-{word}-{word}` where words are lowercase ASCII + digits.
pub fn is_valid_pond_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() != 3 || parts[0] != "pond" {
        return false;
    }
    parts[1..].iter().all(|p| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_sizes() {
        assert_eq!(POND_ADJECTIVES.len(), 64, "need exactly 64 adjectives");
        assert_eq!(POND_NOUNS.len(), 64, "need exactly 64 nouns");
    }

    #[test]
    fn no_duplicate_adjectives() {
        let mut seen = std::collections::HashSet::new();
        for adj in POND_ADJECTIVES {
            assert!(seen.insert(adj), "duplicate adjective: {}", adj);
        }
    }

    #[test]
    fn no_duplicate_nouns() {
        let mut seen = std::collections::HashSet::new();
        for noun in POND_NOUNS {
            assert!(seen.insert(noun), "duplicate noun: {}", noun);
        }
    }

    #[test]
    fn generated_name_format() {
        let name = generate_pond_name();
        assert!(
            name.starts_with("pond-"),
            "must start with 'pond-': {}",
            name
        );
        assert!(
            is_valid_pond_name(&name),
            "generated name must be valid: {}",
            name
        );
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 3, "must have exactly 3 parts: {}", name);
    }

    #[test]
    fn combination_count() {
        let total = POND_ADJECTIVES.len() * POND_NOUNS.len();
        assert!(
            total >= 4096,
            "need at least 4096 combinations, got {}",
            total
        );
    }

    #[test]
    fn no_reserved_names() {
        // Component and entity names that must never appear in pond dictionaries
        let reserved = [
            "moss",
            "rake",
            "cricket",
            "firefly",
            "lantern",
            "probe",
            "koi",
            "stone",
            "pond",
            "garden",
            "beacon",
            "offering",
            "companion",
            "seed",
        ];
        for word in POND_ADJECTIVES.iter().chain(POND_NOUNS.iter()) {
            assert!(
                !reserved.contains(word),
                "reserved name '{}' must not appear in pond dictionary",
                word
            );
        }
    }

    #[test]
    fn validation() {
        assert!(is_valid_pond_name("pond-still-lotus"));
        assert!(is_valid_pond_name("pond-deep-basin"));
        assert!(!is_valid_pond_name("stone-amber-ridge"));
        assert!(!is_valid_pond_name("pond-Still-Lotus"));
        assert!(!is_valid_pond_name("pond-too-many-parts"));
        assert!(!is_valid_pond_name("pond--empty"));
        assert!(!is_valid_pond_name("pond"));
    }
}
