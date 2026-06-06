// crates/kirra-collector/src/bag.rs
//
// The bus side of the join. The collector matches each capture record to a bus
// message (the doer's proposal/trajectory + perception + a doer-version stamp)
// recorded during the same bench run, then references the heavy frames by
// `bulk_ref` rather than copying them (docs/COLLECTOR_DESIGN.md [D2]/[D4]).
//
// [C2] The real bag backend (rosbag2 sqlite3 `.db3` vs MCAP) is DEFERRED until
// the first AWSIM/Autoware session is recorded — the GPU bench isn't up. So bag
// access sits behind the `BagReader` trait; Phase 1 ships an in-memory /
// JSON-backed synthetic impl that exercises the whole join → Parquet →
// reconciliation pipeline with no real bag. A `Db3BagReader` / `McapBagReader`
// slots in later without touching the join or dataset code.

use serde::{Deserialize, Serialize};

/// One bus message the collector can join against. The heavy payload (full
/// trajectory points, object lists, sensor frames) is NOT here — it stays in the
/// bag and is referenced by `bulk_ref`. These are the fields the join needs:
/// the time stamp, the doer version [D3], the cross-check keys [D2], and the
/// reference back into the bag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMessage {
    /// Wall-clock ms of the bus message (the join axis against `t_wall_ms`).
    pub t_wall_ms: u64,
    /// The doer's model/config version, stamped on the bus side [D3]. Kirra is
    /// ignorant of this; the collector attaches it here.
    pub doer_version: String,
    /// Cross-check key [D2] — the asset id, where the bus message carries one.
    #[serde(default)]
    pub asset_id: Option<String>,
    /// Cross-check key [D2] — the trajectory id, where present.
    #[serde(default)]
    pub trajectory_id: Option<u64>,
    /// Cross-check key [D2] — the objects-snapshot freshness stamp, where present.
    #[serde(default)]
    pub objects_ms: Option<u64>,
    /// Reference into the bag for the heavy frames (uri#offset / topic@stamp).
    /// This is the `bulk_ref` written into the Parquet row — never the frames.
    pub bulk_ref: String,
}

/// The result of a successful join: what the collector attaches to the record.
#[derive(Debug, Clone, PartialEq)]
pub struct BusMatch {
    pub doer_version: String,
    pub bulk_ref: String,
    pub matched_t_wall_ms: u64,
}

/// The bus-recording abstraction. The real implementations (db3/MCAP) are a
/// Phase-1 follow-up [C2]; everything upstream depends only on this trait.
pub trait BagReader {
    /// Provenance — the bag's uri (recorded into `bulk_ref` provenance / logs).
    fn bag_uri(&self) -> &str;
    /// All bus messages whose stamp falls within `±window_ms` of `t_wall_ms`.
    /// Order is not guaranteed; the join picks the best candidate.
    fn messages_in_window(&self, t_wall_ms: u64, window_ms: u64) -> Vec<BusMessage>;
}

/// In-memory synthetic bag — Phase 1 tests + offline runs. Loadable from a JSON
/// array of `BusMessage` (the `--bag-json` path) so the binary is runnable
/// end-to-end with no real recording.
pub struct InMemoryBag {
    uri: String,
    messages: Vec<BusMessage>,
}

impl InMemoryBag {
    #[must_use]
    pub fn new(uri: impl Into<String>, messages: Vec<BusMessage>) -> Self {
        Self { uri: uri.into(), messages }
    }

    /// Load from a JSON file containing an array of `BusMessage`.
    pub fn from_json_file(path: &std::path::Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let messages: Vec<BusMessage> = serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self::new(path.display().to_string(), messages))
    }
}

impl BagReader for InMemoryBag {
    fn bag_uri(&self) -> &str {
        &self.uri
    }

    fn messages_in_window(&self, t_wall_ms: u64, window_ms: u64) -> Vec<BusMessage> {
        let lo = t_wall_ms.saturating_sub(window_ms);
        let hi = t_wall_ms.saturating_add(window_ms);
        self.messages
            .iter()
            .filter(|m| m.t_wall_ms >= lo && m.t_wall_ms <= hi)
            .cloned()
            .collect()
    }
}
