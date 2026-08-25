//! The naming well: poetical stone names, `stone-{adjective}-{noun}`.
//!
//! Word lists transcribed from the PoC (`poc/common/src/console/tty.rs`,
//! which kept two platform-variant adjective sets). v1 merges them into
//! one well (L20: the split was an accident of platforms, not a decision);
//! overlap words deduplicated, first-set order preserved.

/// Adjectives — precious materials, gemstones, natural qualities, clarity,
/// color, glasswork.
pub const ADJECTIVES: &[&str] = &[
    // Precious materials
    "amber", "azure", "bronze", "coral", "crimson", "crystal", "emerald", "golden",
    "indigo", "jade", "marble", "obsidian", "pearl", "quartz", "ruby", "silver",
    // Gemstone & mineral
    "topaz", "turquoise", "violet", "onyx", "opal", "garnet", "sapphire", "copper",
    "ivory", "ebony", "platinum", "cobalt", "ochre", "slate", "granite", "basalt",
    // Natural qualities
    "lunar", "solar", "stellar", "misty", "mossy", "frosty", "dusky", "verdant",
    "tranquil", "serene", "gentle", "silent", "ancient", "hidden", "sacred",
    "eternal", "wispy", "shimmering", "glowing", "sunlit", "moonlit", "shadowed",
    "dappled", "veiled", "halcyon", "placid", "limpid", "pristine", "radiant",
    "luminous", "muted", "hushed",
    // Clarity & transparency
    "clear", "lucid", "pellucid", "crystalline", "vitreous", "translucent", "pure",
    "unclouded", "polished", "refined", "flawless", "seamless", "diaphanous",
    "gossamer", "smooth", "still", "calm", "soft", "quiet", "peaceful", "restful",
    "composed", "poised",
    // Color & glasswork
    "vermillion", "cerulean", "scarlet", "teal", "magenta", "gilded", "frosted",
    "stained", "arched", "latticed", "beveled", "etched", "leaded",
];

/// Nouns — landscapes, waters, woodlands, refuges.
pub const NOUNS: &[&str] = &[
    "meadow", "summit", "canyon", "valley", "ridge", "plateau", "basin", "cliff",
    "peak", "dune", "bluff", "mesa", "butte", "hollow", "knoll", "crag", "river",
    "harbor", "glacier", "delta", "stream", "shore", "brook", "lagoon", "spring",
    "cascade", "rapids", "estuary", "inlet", "cove", "fjord", "atoll", "forest",
    "prairie", "desert", "grove", "thicket", "copse", "glade", "heath", "fen",
    "moor", "marsh", "swamp", "taiga", "tundra", "steppe", "savanna", "clearing",
    "alcove", "grotto", "cavern", "ravine", "gorge", "chasm", "vale", "dell",
    "glen", "pass", "garden", "terrace", "oasis", "refuge", "haven",
];

/// Compose one candidate name from two indices (mod list lengths).
/// Callers derive indices from fresh entropy; this function stays pure.
pub fn compose(adj_idx: usize, noun_idx: usize) -> String {
    let adjective = ADJECTIVES[adj_idx % ADJECTIVES.len()];
    let noun = NOUNS[noun_idx % NOUNS.len()];
    format!("stone-{adjective}-{noun}")
}

/// A fallback suffix form when collisions exhaust patience (PoC parity).
pub fn with_hex_suffix(candidate: &str, hex4: &str) -> String {
    format!("{candidate}-{hex4}")
}

#[cfg(test)]
mod tests {
    // R4.1: unwrap/expect sanctioned in tests.
    #![allow(clippy::expect_used)]

    #[test]
    fn composition_follows_the_convention() {
        let idx = |list: &[&str], word: &str| {
            list.iter().position(|w| *w == word).expect("word must be in list")
        };
        assert_eq!(super::compose(idx(super::ADJECTIVES, "emerald"), idx(super::NOUNS, "fen")), "stone-emerald-fen");
        assert_eq!(super::compose(idx(super::ADJECTIVES, "obsidian"), idx(super::NOUNS, "canyon")), "stone-obsidian-canyon");
        assert_eq!(super::compose(idx(super::ADJECTIVES, "leaded"), idx(super::NOUNS, "haven")), "stone-leaded-haven");
    }

    /// L19 pin: the fleet's beloved names must still be mintable.
    #[test]
    fn historic_fleet_names_are_in_the_well() {
        let well = |w: &[&str], word: &str| w.contains(&word);
        assert!(well(super::ADJECTIVES, "leaded"));
        assert!(well(super::ADJECTIVES, "emerald"));
        assert!(well(super::ADJECTIVES, "quartz"));
        assert!(well(super::NOUNS, "fen"));
        assert!(well(super::NOUNS, "vale"));
        assert!(well(super::NOUNS, "summit"));
    }

    #[test]
    fn indices_wrap_without_panicking() {
        let _ = super::compose(usize::MAX, usize::MAX);
    }
}
