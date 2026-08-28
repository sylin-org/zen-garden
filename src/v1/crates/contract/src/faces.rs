//! The HTTP surface, DECLARED (ADR-0009 / B1): every face this contract
//! carries — method, path, and the promise each makes — as DATA. The moss
//! routes and self-describes from this table; clients are verified against
//! it; `surface.json` is emitted from it. A face that is not in this
//! table is not in the garden.

/// One HTTP surface of the garden, by name. The variants ARE the
/// contract's table of contents; everything else about a face is data
/// in [`FACES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    /// Liveness probe of this stone and its wire protocol marker.
    Health,
    /// This route table - every surface, described in place.
    FrontDoor,
    /// Me: my frame, sung full-voice (the SelfView projection).
    StoneSelf,
    /// Me, spelled explicitly (same SelfView).
    StoneThis,
    /// A stone by name or id: mine answered here; others answer 404 with a Location to their home stone (the garden's only true redirect).
    StoneRef,
    /// Local data (L22): this moss's live counters - ingest, dispatch, topology, offerings.
    StonePosture,
    /// Garden data (L22): the room as this moss sees it - self spliced among the peers, every row a canonical frame.
    GardenStones,
    /// The catalog this stone can place from (derived).
    Catalog,
    /// This stone's banks, plus the removable volumes ready for adoption.
    StorageList,
    /// The adopt ceremony: {device: mount point, name: bank FQN} - writes the manifest onto the drive and sings the news (ADR-0005 sec 8).
    StorageAdopt,
    /// Eject a bank by name: authoritative absence, sung to the room (ADR-0005 sec 8.3).
    StorageEject,
    /// Declare a bank's roles: {roles: [sink]} - a sink receives checkpoints (ADR-0005 sec 4).
    StorageRoles,
    /// List a bank directory (optional ?path= subdirectory): the files riding the volume, minus the adoption record. A bank held by a peer answers the garden's redirect (knows_at).
    StorageFileList,
    /// Read one file from a bank: the raw bytes, content-type guessed from the extension; the path is relative to the bank's root. A peer's bank answers the garden's redirect (knows_at).
    StorageFileGet,
    /// Write one file onto a bank: the raw body, parent directories created - makes a sink a real storage destination. A peer's bank answers the garden's redirect (knows_at); writes bind at their authority.
    StorageFilePut,
    /// Delete one file from a bank. Directories refuse - wholesale removal is the operator's hand. A peer's bank answers the garden's redirect.
    StorageFileDelete,
    /// Move (rename) one file within a bank: {move_to: path} - no re-upload. Never overwrites. A peer's bank answers the garden's redirect.
    StorageFileMove,
    /// Follow an offering's logs: history first (tail=N bounds it), then live - SSE `log` events, one JSON line each. A peer's offering answers the garden's redirect.
    OfferingLogsStream,
    /// Run this offering's declared will: Phase A imprint (quiesce -> copy -> resume), then pack, ferry, commit.
    OfferingCapture,
    /// The last capture run of this offering: phase, checkpoint, ferried sinks.
    OfferingCaptureLast,
    /// Replant from a checkpoint {run?}: verify, restore the directory, place from the stored spec - same FQN, same connection strings (ADR-0005 §6).
    OfferingReplant,
    /// Every async operation on this stone, newest first.
    JobList,
    /// One job by id: kind, subject, status, error, result.
    JobDetail,
    /// This stone's living landing page: identity, offerings, banks, the room.
    Portrait,
    /// Lands on the portrait.
    Root,
    /// The live page: stones, offerings, and the event ring as they happen.
    PulsePage,
    /// SSE firehose: topology events (seen/goodbye/expired) and offering changes.
    PulseStream,
    /// Garden data (L22): every bank in the room, self included, from the one cache.
    GardenStorage,
    /// Every offering placed on this stone (the collection).
    OfferingList,
    /// Plant a managed offering {image?, ports:{name:container}, runtime?, inputs?}; catalog name wins when one exists.
    OfferingPlant,
    /// The placed record - plan, decisions, ports (OFFERINGS.md §5.3).
    OfferingShow,
    /// Rest a managed offering - stopped, and reconcile will keep it so.
    OfferingRest,
    /// Wake a rested offering; resurrects from its stored spec if reality lost it.
    OfferingWake,
    /// Uproot - remove the workload and forget the offering.
    OfferingUproot,
}

/// A face's declaration: the wire verb, the path, and the promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaceDef {
    pub face: Face,
    pub method: &'static str,
    pub path: &'static str,
    pub summary: &'static str,
}

/// Every face, in contract order. THE table (L9 pointed at the wire):
/// if a route is not here, it must not exist anywhere.
pub const FACES: &[FaceDef] = &[
    FaceDef { face: Face::Health, method: "GET", path: "/health", summary: "Liveness probe of this stone and its wire protocol marker." },
    FaceDef { face: Face::FrontDoor, method: "GET", path: "/api/v1", summary: "This route table - every surface, described in place." },
    FaceDef { face: Face::StoneSelf, method: "GET", path: "/api/v1/stone", summary: "Me: my frame, sung full-voice (the SelfView projection)." },
    FaceDef { face: Face::StoneThis, method: "GET", path: "/api/v1/stone/this", summary: "Me, spelled explicitly (same SelfView)." },
    FaceDef { face: Face::StoneRef, method: "GET", path: "/api/v1/stone/{ref}", summary: "A stone by name or id: mine answered here; others answer 404 with a Location to their home stone (the garden's only true redirect)." },
    FaceDef { face: Face::StonePosture, method: "GET", path: "/api/v1/stone/posture", summary: "Local data (L22): this moss's live counters - ingest, dispatch, topology, offerings." },
    FaceDef { face: Face::GardenStones, method: "GET", path: "/api/v1/garden/stones", summary: "Garden data (L22): the room as this moss sees it - self spliced among the peers, every row a canonical frame." },
    FaceDef { face: Face::Catalog, method: "GET", path: "/api/v1/catalog", summary: "The catalog this stone can place from (derived)." },
    FaceDef { face: Face::StorageList, method: "GET", path: "/api/v1/storage", summary: "This stone's banks, plus the removable volumes ready for adoption." },
    FaceDef { face: Face::StorageAdopt, method: "POST", path: "/api/v1/storage/adopt", summary: "The adopt ceremony: {device: mount point, name: bank FQN} - writes the manifest onto the drive and sings the news (ADR-0005 sec 8)." },
    FaceDef { face: Face::StorageEject, method: "POST", path: "/api/v1/storage/{fqn}/eject", summary: "Eject a bank by name: authoritative absence, sung to the room (ADR-0005 sec 8.3)." },
    FaceDef { face: Face::StorageRoles, method: "POST", path: "/api/v1/storage/{fqn}/roles", summary: "Declare a bank's roles: {roles: [sink]} - a sink receives checkpoints (ADR-0005 sec 4)." },
    FaceDef { face: Face::StorageFileList, method: "GET", path: "/api/v1/storage/{fqn}/files", summary: "List a bank directory (optional ?path= subdirectory): the files riding the volume, minus the adoption record. A bank held by a peer answers the garden's redirect (knows_at)." },
    FaceDef { face: Face::StorageFileGet, method: "GET", path: "/api/v1/storage/{fqn}/files/{*path}", summary: "Read one file from a bank: the raw bytes, content-type guessed from the extension; the path is relative to the bank's root. A peer's bank answers the garden's redirect (knows_at)." },
    FaceDef { face: Face::StorageFilePut, method: "PUT", path: "/api/v1/storage/{fqn}/files/{*path}", summary: "Write one file onto a bank: the raw body, parent directories created - makes a sink a real storage destination. A peer's bank answers the garden's redirect (knows_at); writes bind at their authority." },
    FaceDef { face: Face::StorageFileDelete, method: "DELETE", path: "/api/v1/storage/{fqn}/files/{*path}", summary: "Delete one file from a bank. Directories refuse - wholesale removal is the operator's hand. A peer's bank answers the garden's redirect." },
    FaceDef { face: Face::StorageFileMove, method: "PATCH", path: "/api/v1/storage/{fqn}/files/{*path}", summary: "Move (rename) one file within a bank: {move_to: path} - no re-upload. Never overwrites. A peer's bank answers the garden's redirect." },
    FaceDef { face: Face::OfferingLogsStream, method: "GET", path: "/api/v1/offerings/{fqn}/logs/stream", summary: "Follow an offering's logs: history first (tail=N bounds it), then live - SSE `log` events, one JSON line each. A peer's offering answers the garden's redirect." },
    FaceDef { face: Face::OfferingCapture, method: "POST", path: "/api/v1/offerings/{fqn}/capture", summary: "Run this offering's declared will: Phase A imprint (quiesce -> copy -> resume), then pack, ferry, commit." },
    FaceDef { face: Face::OfferingCaptureLast, method: "GET", path: "/api/v1/offerings/{fqn}/capture", summary: "The last capture run of this offering: phase, checkpoint, ferried sinks." },
    FaceDef { face: Face::OfferingReplant, method: "POST", path: "/api/v1/offerings/{fqn}/replant", summary: "Replant from a checkpoint {run?}: verify, restore the directory, place from the stored spec - same FQN, same connection strings (ADR-0005 §6)." },
    FaceDef { face: Face::JobList, method: "GET", path: "/api/v1/jobs", summary: "Every async operation on this stone, newest first." },
    FaceDef { face: Face::JobDetail, method: "GET", path: "/api/v1/jobs/{id}", summary: "One job by id: kind, subject, status, error, result." },
    FaceDef { face: Face::Portrait, method: "GET", path: "/portrait", summary: "This stone's living landing page: identity, offerings, banks, the room." },
    FaceDef { face: Face::Root, method: "GET", path: "/", summary: "Lands on the portrait." },
    FaceDef { face: Face::PulsePage, method: "GET", path: "/pulse", summary: "The live page: stones, offerings, and the event ring as they happen." },
    FaceDef { face: Face::PulseStream, method: "GET", path: "/pulse/stream", summary: "SSE firehose: topology events (seen/goodbye/expired) and offering changes." },
    FaceDef { face: Face::GardenStorage, method: "GET", path: "/api/v1/garden/storage", summary: "Garden data (L22): every bank in the room, self included, from the one cache." },
    FaceDef { face: Face::OfferingList, method: "GET", path: "/api/v1/offerings", summary: "Every offering placed on this stone (the collection)." },
    FaceDef { face: Face::OfferingPlant, method: "POST", path: "/api/v1/offerings/{fqn}", summary: "Plant a managed offering {image?, ports:{name:container}, runtime?, inputs?}; catalog name wins when one exists." },
    FaceDef { face: Face::OfferingShow, method: "GET", path: "/api/v1/offerings/{fqn}", summary: "The placed record - plan, decisions, ports (OFFERINGS.md §5.3)." },
    FaceDef { face: Face::OfferingRest, method: "POST", path: "/api/v1/offerings/{fqn}/rest", summary: "Rest a managed offering - stopped, and reconcile will keep it so." },
    FaceDef { face: Face::OfferingWake, method: "POST", path: "/api/v1/offerings/{fqn}/wake", summary: "Wake a rested offering; resurrects from its stored spec if reality lost it." },
    FaceDef { face: Face::OfferingUproot, method: "DELETE", path: "/api/v1/offerings/{fqn}", summary: "Uproot - remove the workload and forget the offering." },
];

impl Face {
    /// This face's declaration. Total: every variant has exactly one row
    /// (pinned by the bijection test below).
    pub fn def(self) -> &'static FaceDef {
        let idx = FACES.iter().position(|d| d.face == self).unwrap_or(0);
        &FACES[idx]
    }

    #[cfg(test)]
    fn has_def(self) -> bool {
        FACES.iter().any(|d| d.face == self)
    }

    /// The wire verb.
    pub fn method(self) -> &'static str {
        self.def().method
    }

    /// The path template (`{fqn}`, `{id}`, `{*path}` are placeholders).
    pub fn path(self) -> &'static str {
        self.def().path
    }

    /// The promise this face makes (rendered by the front door).
    pub fn summary(self) -> &'static str {
        self.def().summary
    }

    /// Every face, in contract order.
    pub fn all() -> impl Iterator<Item = Face> {
        FACES.iter().map(|d| d.face)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Every variant has exactly one declaration: the enum and the table
    /// are the same set, with no duplicates (the drift gate).
    #[test]
    fn faces_and_table_are_a_bijection() {
        for face in Face::all() {
            assert!(face.has_def(), "{face:?} missing from FACES");
        }
        let with_defs = Face::all().filter(|f| f.has_def()).count();
        assert_eq!(with_defs, FACES.len(), "one declaration per face");
        let mut paths: Vec<_> = FACES.iter().map(|d| (d.method, d.path)).collect();
        let count = paths.len();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), count, "no duplicate (method, path) rows");
    }

    /// The declaration table's paths all start at the root and speak the
    /// v1 grammar (the PoC's ghost paths are dead — L9).
    #[test]
    fn every_face_path_is_absolute() {
        for d in FACES {
            assert!(d.path.starts_with('/'), "{:?} path must be absolute", d.face);
        }
    }
}
