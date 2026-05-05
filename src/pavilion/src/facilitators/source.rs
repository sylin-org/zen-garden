//! v0 suggestion sources. Each function takes a snapshot of the
//! observed state and returns either a `Suggestion` or `None`.
//!
//! Sources are pure of state mutation — the engine handles the
//! ordering, dismissal lookup, and emit cadence. That keeps each
//! source small and easy to test in isolation.

use crate::awareness::AwareStone;
use crate::tending::TendedStone;

use super::types::{Suggestion, SuggestionAction};

/// Snapshot of garden state the engine passes to every source.
/// All fields are owned clones so a source can run on a worker
/// thread without holding any locks.
pub struct SuggestionContext {
    pub stones: Vec<AwareStone>,
    pub tended: Option<TendedStone>,
    /// Whether the tended stone reports a pond initialised.
    /// `None` when no stone is tended, or when the status fetch
    /// failed.
    pub pond_initialised: Option<bool>,
}

/// "Tend a stone to start" — fires when at least one stone is in
/// awareness and Pavilion has no tending. Picks the first stone
/// in the snapshot (which `Awareness::snapshot` orders by
/// first-seen ascending), so the user gets a stable target.
pub fn tend_a_stone(ctx: &SuggestionContext) -> Option<Suggestion> {
    if ctx.tended.is_some() {
        return None;
    }
    let target = ctx.stones.first()?;
    Some(Suggestion {
        id: format!("tend_a_stone:{}", target.stone_id),
        kind: "facilitator:tend_a_stone".to_string(),
        title: "Tend a stone to start".to_string(),
        body: format!(
            "Pavilion is hearing {} stone{} on the network but isn't anchored to one yet. \
             Tending {} would let you see its services, banks, and pond status.",
            ctx.stones.len(),
            if ctx.stones.len() == 1 { "" } else { "s" },
            target.stone_name,
        ),
        action_label: format!("Tend {}", target.stone_name),
        action: SuggestionAction::Tend {
            stone_id: target.stone_id.clone(),
            stone_name: target.stone_name.clone(),
        },
    })
}

/// "Set up pond security" — fires when 2+ stones are aware and the
/// tended stone has no pond. Opens the Pond destination so the
/// user can read up before invoking `garden-rake pond init`.
///
/// Once pond ceremonies land in Pavilion (M2), the action will
/// switch to opening the init modal.
pub fn enable_pond(ctx: &SuggestionContext) -> Option<Suggestion> {
    if ctx.stones.len() < 2 {
        return None;
    }
    let tended = ctx.tended.as_ref()?;
    // Only fire when we have a definite "no pond" signal — fetch
    // failures show as `None` and shouldn't trigger a suggestion.
    if ctx.pond_initialised != Some(false) {
        return None;
    }
    Some(Suggestion {
        id: format!("enable_pond:{}", tended.stone_name),
        kind: "facilitator:enable_pond".to_string(),
        title: "Set up pond security?".to_string(),
        body: format!(
            "You have {} stones in your garden but {} has no pond yet. \
             A pond binds the stones to a shared identity so cross-stone \
             operations stay trusted.",
            ctx.stones.len(),
            tended.stone_name,
        ),
        action_label: "Open Pond".to_string(),
        action: SuggestionAction::OpenView {
            view: "pond".to_string(),
        },
    })
}

/// Run every source in order, return the first non-`None`. v0
/// uses static priority (tend-first, pond-second); a richer
/// ranking can land later.
pub fn pick(ctx: &SuggestionContext) -> Option<Suggestion> {
    tend_a_stone(ctx).or_else(|| enable_pond(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn aware(name: &str, id: &str) -> AwareStone {
        AwareStone {
            stone_id: id.to_string(),
            stone_name: name.to_string(),
            endpoint: format!("http://{name}:7185"),
            health: "thriving".to_string(),
            services_count: 0,
            last_seen: Utc::now(),
            age_secs: 0,
            seen_first_secs: 0,
        }
    }

    fn tended(name: &str) -> TendedStone {
        TendedStone {
            stone_name: name.to_string(),
            endpoint: format!("http://{name}:7185"),
        }
    }

    #[test]
    fn tend_a_stone_fires_when_aware_but_untended() {
        let ctx = SuggestionContext {
            stones: vec![aware("alpha", "id-a")],
            tended: None,
            pond_initialised: None,
        };
        let s = tend_a_stone(&ctx).expect("must fire");
        assert_eq!(s.kind, "facilitator:tend_a_stone");
        assert!(matches!(s.action, SuggestionAction::Tend { .. }));
    }

    #[test]
    fn tend_a_stone_quiet_when_already_tended() {
        let ctx = SuggestionContext {
            stones: vec![aware("alpha", "id-a")],
            tended: Some(tended("alpha")),
            pond_initialised: Some(true),
        };
        assert!(tend_a_stone(&ctx).is_none());
    }

    #[test]
    fn tend_a_stone_quiet_when_no_stones_aware() {
        let ctx = SuggestionContext {
            stones: vec![],
            tended: None,
            pond_initialised: None,
        };
        assert!(tend_a_stone(&ctx).is_none());
    }

    #[test]
    fn enable_pond_fires_with_2_stones_and_no_pond() {
        let ctx = SuggestionContext {
            stones: vec![aware("alpha", "id-a"), aware("beta", "id-b")],
            tended: Some(tended("alpha")),
            pond_initialised: Some(false),
        };
        let s = enable_pond(&ctx).expect("must fire");
        assert_eq!(s.kind, "facilitator:enable_pond");
        assert!(matches!(
            &s.action,
            SuggestionAction::OpenView { view } if view == "pond"
        ));
    }

    #[test]
    fn enable_pond_quiet_with_pond_already_initialised() {
        let ctx = SuggestionContext {
            stones: vec![aware("alpha", "id-a"), aware("beta", "id-b")],
            tended: Some(tended("alpha")),
            pond_initialised: Some(true),
        };
        assert!(enable_pond(&ctx).is_none());
    }

    #[test]
    fn enable_pond_quiet_when_pond_status_unknown() {
        // Fetch-failure case shouldn't trigger a misleading
        // suggestion.
        let ctx = SuggestionContext {
            stones: vec![aware("alpha", "id-a"), aware("beta", "id-b")],
            tended: Some(tended("alpha")),
            pond_initialised: None,
        };
        assert!(enable_pond(&ctx).is_none());
    }

    #[test]
    fn enable_pond_quiet_under_two_stones() {
        let ctx = SuggestionContext {
            stones: vec![aware("alpha", "id-a")],
            tended: Some(tended("alpha")),
            pond_initialised: Some(false),
        };
        assert!(enable_pond(&ctx).is_none());
    }

    #[test]
    fn pick_prefers_tend_over_pond() {
        // Both could fire (no tending and 2+ stones); tend should
        // win because it's more fundamental — you can't pond
        // without tending first.
        let ctx = SuggestionContext {
            stones: vec![aware("alpha", "id-a"), aware("beta", "id-b")],
            tended: None,
            pond_initialised: Some(false),
        };
        let s = pick(&ctx).expect("must pick");
        assert_eq!(s.kind, "facilitator:tend_a_stone");
    }
}
