//! Dynamic Symbol Blocklist — §29.
//! Loaded at startup from configs/blocklist.toml.
//! Reloaded every 60 seconds via polling (no restart required).
//! Entries with expiry < now are automatically purged.

use core_types::SymbolId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocklistEntry {
    pub symbol: String,
    pub reason: String,
    pub date_added: String,
    pub expiry: Option<String>, // ISO date "YYYY-MM-DD" or absent = permanent
    pub auto_added: bool,
}

#[derive(Debug, Deserialize)]
struct BlocklistFile {
    #[serde(default)]
    symbols: Vec<BlocklistEntry>,
}

#[derive(Debug)]
pub struct Blocklist {
    path: PathBuf,
    entries: HashMap<String, BlocklistEntry>, // ticker → entry
    blocked_ids: HashSet<SymbolId>,
    ticker_to_id: HashMap<String, SymbolId>,
    last_loaded: SystemTime,
    reload_interval: Duration,
}

impl Blocklist {
    pub fn new(path: impl Into<PathBuf>, reload_secs: u64) -> Self {
        let mut bl = Self {
            path: path.into(),
            entries: HashMap::new(),
            blocked_ids: HashSet::new(),
            ticker_to_id: HashMap::new(),
            last_loaded: SystemTime::UNIX_EPOCH,
            reload_interval: Duration::from_secs(reload_secs),
        };
        bl.reload_if_needed();
        bl
    }

    /// Register a ticker → SymbolId mapping (called when a symbol enters Tier B/C).
    pub fn register_symbol(&mut self, ticker: String, id: SymbolId) {
        self.ticker_to_id.insert(ticker, id);
        self.rebuild_id_set();
    }

    /// Returns true if `id` is currently blocked.
    ///
    /// # Example
    /// ```
    /// use risk_engine::blocklist::Blocklist;
    /// use core_types::SymbolId;
    /// let mut bl = Blocklist::new("dummy.toml", 60);
    /// bl.register_symbol("XYZ".to_string(), SymbolId(10));
    /// bl.auto_block("XYZ".to_string(), "Delisting".to_string());
    /// assert!(bl.is_blocked(SymbolId(10)));
    /// ```
    pub fn is_blocked(&self, id: SymbolId) -> bool {
        self.blocked_ids.contains(&id)
    }

    /// Call periodically — reloads file if reload_interval has passed.
    pub fn reload_if_needed(&mut self) {
        if self.last_loaded.elapsed().unwrap_or(self.reload_interval) >= self.reload_interval {
            self.load();
        }
    }

    /// Adds a symbol to the blocklist programmatically (e.g., from IBKR event).
    pub fn auto_block(&mut self, ticker: String, reason: String) {
        let entry = BlocklistEntry {
            symbol: ticker.clone(),
            reason,
            date_added: chrono::Utc::now().format("%Y-%m-%d").to_string(),
            expiry: None,
            auto_added: true,
        };
        self.entries.insert(ticker, entry);
        self.rebuild_id_set();
        log::warn!("AUTO-BLOCKED: symbol added to blocklist");
    }

    fn load(&mut self) {
        if !self.path.exists() {
            log::warn!(
                "Blocklist file not found: {:?} — using empty list",
                self.path
            );
            self.last_loaded = SystemTime::now();
            return;
        }
        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                match toml::from_str::<BlocklistFile>(&content) {
                    Ok(file) => {
                        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
                        // Filter out expired entries
                        let active: HashMap<String, BlocklistEntry> = file
                            .symbols
                            .into_iter()
                            .filter(|e| {
                                let expired =
                                    e.expiry.as_ref().map(|exp| exp <= &today).unwrap_or(false);
                                if expired {
                                    log::info!("BLOCKLIST_EXPIRED_REMOVED: {}", e.symbol);
                                }
                                !expired
                            })
                            .map(|e| (e.symbol.clone(), e))
                            .collect();
                        self.entries = active;
                        self.rebuild_id_set();
                        self.last_loaded = SystemTime::now();
                        log::debug!("Blocklist loaded: {} active entries", self.entries.len());
                    }
                    Err(e) => log::error!("Failed to parse blocklist.toml: {}", e),
                }
            }
            Err(e) => log::error!("Failed to read blocklist.toml: {}", e),
        }
    }

    fn rebuild_id_set(&mut self) {
        self.blocked_ids = self
            .entries
            .keys()
            .filter_map(|ticker| self.ticker_to_id.get(ticker))
            .copied()
            .collect();
    }
}
