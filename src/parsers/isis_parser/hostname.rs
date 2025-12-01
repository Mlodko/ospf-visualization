#![allow(unused)]
use std::collections::HashMap;

use crate::parsers::isis_parser::core_lsp::SystemId;

#[derive(Debug, Clone)]
pub struct HostnameEntry {
    pub hostname: String,
    pub system_id: SystemId,
    pub is_local: bool,
}

impl HostnameEntry {
    /// Parse a single line from `show isis hostname` and produce a `HostnameEntry`.
    ///
    /// Expected example lines:
    /// - `1      0000.0000.0004 a47b41368a00`
    /// - `     * 0000.0000.0001 e3f5f5af05f6`
    ///
    /// The parser is tolerant of an optional leading level number and/or a leading `*`.
    /// It treats the last whitespace-separated token as the hostname and the token
    /// before that as the System ID (which is validated via `SystemId::from_string`).
    pub fn parse(line: &str) -> Result<Self, String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err("empty line".to_string());
        }

        // Tokenize on whitespace to be resilient to spacing
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.len() < 2 {
            return Err(format!("not enough tokens in line: {}", line));
        }

        // If any token equals "*", mark as local/own entry
        let is_local = tokens.iter().any(|t| *t == "*");

        // Last token = hostname, penultimate token = system id
        let hostname_tok = tokens[tokens.len() - 1];
        let system_id_tok = tokens[tokens.len() - 2];

        // Validate system ID using existing SystemId parser
        let system_id = SystemId::from_string(system_id_tok)
            .map_err(|e| format!("invalid system id '{}' in line '{}': {:?}", system_id_tok, line, e))?;

        Ok(HostnameEntry {
            hostname: hostname_tok.to_string(),
            system_id,
            is_local,
        })
    }
}

/// Dual-index map for hostnames and system IDs.
/// - `map_by_hostname` is keyed by the dynamic hostname string as printed in `show isis hostname`.
///   Use this when an LSP contains only the hostname.
/// - `map_by_system_id` is keyed by the canonical `SystemId`
///   Use this when you already have the system id and want the hostname or other metadata.
#[derive(Debug, Clone)]
pub struct HostnameMap {
    map_by_hostname: HashMap<String, HostnameEntry>,
    map_by_system_id: HashMap<SystemId, HostnameEntry>,
}

impl HostnameMap {
    /// Build a HostnameMap from an iterator of lines (e.g. the entire command output split by '\n').
    /// Non-data lines (headers, empty lines) are ignored.
    /// If multiple entries share the same hostname or system id, the last parsed line wins.
    pub fn build_map_from_lines<I>(lines: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let mut by_name: HashMap<String, HostnameEntry> = HashMap::new();
        let mut by_sys: HashMap<SystemId, HostnameEntry> = HashMap::new();

        for line in lines {
            let s = line.as_ref();
            match HostnameEntry::parse(s) {
                Ok(entry) => {
                    // Insert into both maps; clone as needed
                    by_name.insert(entry.hostname.clone(), entry.clone());
                    by_sys.insert(entry.system_id.clone(), entry);
                }
                Err(_) => {
                    // ignore unparseable lines (headers, blank lines, etc.)
                    continue;
                }
            }
        }

        Self {
            map_by_hostname: by_name,
            map_by_system_id: by_sys,
        }
    }

    /// Insert or overwrite an entry. Keeps both maps in sync.
    pub fn insert(&mut self, entry: HostnameEntry) {
        self.map_by_hostname
            .insert(entry.hostname.clone(), entry.clone());
        self.map_by_system_id.insert(entry.system_id.clone(), entry);
    }
    
    pub fn len(&self) -> usize {
        self.map_by_hostname.len()
    }

    /// Lookup by hostname string (fast path for LSPs that carry hostname).
    pub fn get_by_hostname(&self, hostname: &str) -> Option<&HostnameEntry> {
        self.map_by_hostname.get(hostname)
    }
    
    pub fn get_system_id_by_hostname(&self, hostname: &str) -> Option<&SystemId> {
        self.map_by_hostname.get(hostname).map(|entry| &entry.system_id)
    }

    /// Lookup by SystemId.
    pub fn get_by_system_id(&self, system_id: &SystemId) -> Option<&HostnameEntry> {
        self.map_by_system_id.get(system_id)
    }

    /// Returns true if we have an entry for the given hostname.
    pub fn contains_hostname(&self, hostname: &str) -> bool {
        self.map_by_hostname.contains_key(hostname)
    }
    
    pub fn iter_entries(&self) -> impl Iterator<Item = &HostnameEntry> {
        self.map_by_hostname.values()
    }
}

mod tests {
    use super::*;
    
    #[test]
    fn test_parse_hostname_map() {
        let input = include_str!("../../../test_data/isis_hostname_map_input.txt");
        let map = HostnameMap::build_map_from_lines(input.lines());
        
        assert_eq!(map.len(), 8);
        assert_eq!(map.get_system_id_by_hostname("r1").unwrap(), &SystemId::from_string("0000.0000.0001").unwrap());
    }
}
