//! Trigram (3-byte sequence) inverted index for fast text search.
//!
//! Builds an in-memory index of all trigrams found in text files under a root
//! directory. Queries extract trigrams from the search string, intersect posting
//! lists to find candidate files, then verify with actual content scanning.
//!
//! Uses `ignore::WalkBuilder` for `.gitignore`-aware traversal.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Public types ──────────────────────────────────────────────────────

/// A trigram inverted index over text files in a directory tree.
///
/// Uses a custom serialization format because `HashMap<[u8; 3], _>` cannot
/// be directly serialized to JSON (byte-array keys are not valid JSON keys).
/// We convert to a flat list of `(trigram_hex, postings)` pairs on save.
pub struct NgramIndex {
    /// trigram -> list of (file_id, occurrence count)
    index: HashMap<[u8; 3], Vec<(u32, u16)>>,
    /// Ordered list of indexed file paths (index into this via file_id).
    files: Vec<PathBuf>,
    /// Byte size of each file at index time.
    file_sizes: Vec<u64>,
    /// When the index was built/last updated.
    built_at_epoch_ms: u64,
}

/// Wire format for serialization (trigram as hex string key).
#[derive(Serialize, Deserialize)]
struct NgramIndexWire {
    /// Each entry is (hex_trigram, postings).
    entries: Vec<(String, Vec<(u32, u16)>)>,
    files: Vec<PathBuf>,
    file_sizes: Vec<u64>,
    built_at_epoch_ms: u64,
}

/// A single search result from the index.
#[derive(Debug, Clone)]
pub struct NgramSearchResult {
    pub path: PathBuf,
    pub match_count: usize,
}

/// Summary statistics about an index.
#[derive(Debug, Clone, Serialize)]
pub struct IndexStats {
    pub file_count: usize,
    pub trigram_count: usize,
    pub index_size_bytes: u64,
    pub built_at_epoch_ms: u64,
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Extract all unique trigrams from a byte slice.
fn extract_trigrams(data: &[u8]) -> HashMap<[u8; 3], u16> {
    let mut trigrams: HashMap<[u8; 3], u16> = HashMap::new();
    if data.len() < 3 {
        return trigrams;
    }
    for window in data.windows(3) {
        let key: [u8; 3] = [window[0], window[1], window[2]];
        let count = trigrams.entry(key).or_insert(0);
        *count = count.saturating_add(1);
    }
    trigrams
}

/// Check if a file appears to be binary by scanning the first 512 bytes for
/// null bytes.
fn is_likely_binary(data: &[u8]) -> bool {
    let check_len = data.len().min(512);
    data[..check_len].contains(&0)
}

/// Current time as milliseconds since Unix epoch.
fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Collect text files under `root`, respecting `.gitignore`.
fn walk_text_files(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true) // skip dotfiles/dirs
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker.flatten() {
        if entry.file_type().map_or(true, |ft| !ft.is_file()) {
            continue;
        }
        paths.push(entry.into_path());
    }
    paths
}

/// Read file content. Returns None if the file cannot be read.
fn read_file_bytes(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

// ── Implementation ────────────────────────────────────────────────────

impl NgramIndex {
    /// Build a new index from all text files under `root`.
    ///
    /// Respects `.gitignore` via the `ignore` crate. Skips binary files
    /// (detected by null bytes in the first 512 bytes).
    pub fn build(root: &Path) -> Result<Self, std::io::Error> {
        let paths = walk_text_files(root);
        let mut index: HashMap<[u8; 3], Vec<(u32, u16)>> = HashMap::new();
        let mut files: Vec<PathBuf> = Vec::new();
        let mut file_sizes: Vec<u64> = Vec::new();

        for path in paths {
            let data = match read_file_bytes(&path) {
                Some(d) => d,
                None => continue,
            };

            if is_likely_binary(&data) {
                continue;
            }

            let file_id = files.len() as u32;
            let size = data.len() as u64;
            files.push(path);
            file_sizes.push(size);

            let trigrams = extract_trigrams(&data);
            for (tri, count) in trigrams {
                index.entry(tri).or_default().push((file_id, count));
            }
        }

        Ok(Self {
            index,
            files,
            file_sizes,
            built_at_epoch_ms: now_epoch_ms(),
        })
    }

    /// Incrementally update the index for a set of changed file paths.
    ///
    /// Removes old entries for those files and re-indexes them. Files that
    /// no longer exist are removed from the index.
    pub fn update(&mut self, changed: &[PathBuf]) -> Result<(), std::io::Error> {
        // Build a set of changed canonical paths for fast lookup
        let changed_set: std::collections::HashSet<PathBuf> = changed
            .iter()
            .filter_map(|p| std::fs::canonicalize(p).ok())
            .collect();

        // Find file_ids that need re-indexing
        let mut ids_to_remove: Vec<u32> = Vec::new();
        for (id, path) in self.files.iter().enumerate() {
            if let Ok(canon) = std::fs::canonicalize(path) {
                if changed_set.contains(&canon) {
                    ids_to_remove.push(id as u32);
                }
            }
        }

        // Remove old posting-list entries for those file_ids
        let remove_set: std::collections::HashSet<u32> =
            ids_to_remove.iter().copied().collect();
        if !remove_set.is_empty() {
            self.index.retain(|_tri, postings| {
                postings.retain(|(fid, _)| !remove_set.contains(fid));
                !postings.is_empty()
            });
        }

        // Re-index changed files that still exist
        for path in changed {
            if !path.is_file() {
                continue;
            }
            let data = match read_file_bytes(path) {
                Some(d) => d,
                None => continue,
            };
            if is_likely_binary(&data) {
                continue;
            }

            // Check if this file already has an id
            let file_id = if let Some(pos) = self.files.iter().position(|p| p == path) {
                self.file_sizes[pos] = data.len() as u64;
                pos as u32
            } else {
                let id = self.files.len() as u32;
                self.files.push(path.clone());
                self.file_sizes.push(data.len() as u64);
                id
            };

            let trigrams = extract_trigrams(&data);
            for (tri, count) in trigrams {
                self.index.entry(tri).or_default().push((file_id, count));
            }
        }

        self.built_at_epoch_ms = now_epoch_ms();
        Ok(())
    }

    /// Search the index for files containing `query`.
    ///
    /// Extracts trigrams from the query, intersects posting lists to find
    /// candidate files, then verifies with actual substring search on candidates.
    /// Returns up to `max_results` matches sorted by match count descending.
    pub fn search(&self, query: &str, max_results: usize) -> Vec<NgramSearchResult> {
        let query_bytes = query.as_bytes();
        if query_bytes.len() < 3 {
            // For very short queries, fall back to brute-force scan of all files
            return self.brute_force_search(query, max_results);
        }

        // Extract query trigrams (unique set)
        let query_trigrams = extract_trigrams(query_bytes);
        let tri_keys: Vec<[u8; 3]> = query_trigrams.keys().copied().collect();

        if tri_keys.is_empty() {
            return Vec::new();
        }

        // Intersect posting lists: find files that contain ALL query trigrams.
        // Start with the shortest posting list for efficiency.
        let mut sorted_lists: Vec<&Vec<(u32, u16)>> = tri_keys
            .iter()
            .filter_map(|tri| self.index.get(tri))
            .collect();

        if sorted_lists.len() != tri_keys.len() {
            // Some trigram not in index at all -> no matches
            return Vec::new();
        }

        sorted_lists.sort_by_key(|list| list.len());

        // Start with file_ids from the smallest posting list
        let mut candidates: HashMap<u32, usize> = HashMap::new();
        for &(fid, count) in sorted_lists[0] {
            candidates.insert(fid, count as usize);
        }

        // Intersect with remaining lists
        for postings in &sorted_lists[1..] {
            let posting_set: HashMap<u32, u16> =
                postings.iter().copied().collect();
            candidates.retain(|fid, score| {
                if let Some(count) = posting_set.get(fid) {
                    *score += *count as usize;
                    true
                } else {
                    false
                }
            });
            if candidates.is_empty() {
                return Vec::new();
            }
        }

        // Verify candidates by actually searching file content
        let mut results: Vec<NgramSearchResult> = Vec::new();
        for (fid, _score) in &candidates {
            let path = &self.files[*fid as usize];
            let data = match read_file_bytes(path) {
                Some(d) => d,
                None => continue,
            };

            // Count actual occurrences of query in file content
            let match_count = count_substring_occurrences(&data, query_bytes);
            if match_count > 0 {
                results.push(NgramSearchResult {
                    path: path.clone(),
                    match_count,
                });
            }
        }

        // Sort by match count descending
        results.sort_by(|a, b| b.match_count.cmp(&a.match_count));
        results.truncate(max_results);
        results
    }

    /// Brute-force search for very short queries (< 3 bytes) that can't use
    /// trigram intersection.
    fn brute_force_search(&self, query: &str, max_results: usize) -> Vec<NgramSearchResult> {
        let query_bytes = query.as_bytes();
        let mut results: Vec<NgramSearchResult> = Vec::new();

        for path in &self.files {
            let data = match read_file_bytes(path) {
                Some(d) => d,
                None => continue,
            };
            let match_count = count_substring_occurrences(&data, query_bytes);
            if match_count > 0 {
                results.push(NgramSearchResult {
                    path: path.clone(),
                    match_count,
                });
            }
        }

        results.sort_by(|a, b| b.match_count.cmp(&a.match_count));
        results.truncate(max_results);
        results
    }

    /// Save the index to a file as JSON.
    ///
    /// Converts the in-memory `HashMap<[u8; 3], _>` to a serializable wire
    /// format with hex-encoded trigram keys.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let wire = NgramIndexWire {
            entries: self
                .index
                .iter()
                .map(|(tri, postings)| {
                    let hex = format!("{:02x}{:02x}{:02x}", tri[0], tri[1], tri[2]);
                    (hex, postings.clone())
                })
                .collect(),
            files: self.files.clone(),
            file_sizes: self.file_sizes.clone(),
            built_at_epoch_ms: self.built_at_epoch_ms,
        };

        let serialized = serde_json::to_vec(&wire).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, format!("serialize error: {e}"))
        })?;

        // Write atomically via temp file
        let tmp_path = path.with_extension("tmp");
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(&serialized)?;
        file.sync_all()?;
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    }

    /// Load the index from a file.
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let mut file = std::fs::File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        let wire: NgramIndexWire = serde_json::from_slice(&data).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("deserialize error: {e}"))
        })?;

        let mut index: HashMap<[u8; 3], Vec<(u32, u16)>> = HashMap::new();
        for (hex, postings) in wire.entries {
            if hex.len() == 6 {
                if let (Ok(a), Ok(b), Ok(c)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    index.insert([a, b, c], postings);
                }
            }
        }

        Ok(Self {
            index,
            files: wire.files,
            file_sizes: wire.file_sizes,
            built_at_epoch_ms: wire.built_at_epoch_ms,
        })
    }

    /// Return summary statistics about this index.
    pub fn stats(&self) -> IndexStats {
        // Estimate in-memory size: trigrams * (3 key + vec overhead) + postings * 6
        let postings_count: usize = self.index.values().map(|v| v.len()).sum();
        let estimated_bytes = (self.index.len() * (3 + 24)) + (postings_count * 6);

        IndexStats {
            file_count: self.files.len(),
            trigram_count: self.index.len(),
            index_size_bytes: estimated_bytes as u64,
            built_at_epoch_ms: self.built_at_epoch_ms,
        }
    }
}

/// Count non-overlapping occurrences of `needle` in `haystack`.
fn count_substring_occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    let mut count = 0;
    let mut start = 0;
    while start + needle.len() <= haystack.len() {
        if &haystack[start..start + needle.len()] == needle {
            count += 1;
            start += needle.len(); // non-overlapping
        } else {
            start += 1;
        }
    }
    count
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_trigrams() {
        let data = b"hello";
        let tris = extract_trigrams(data);
        // "hello" -> "hel", "ell", "llo"
        assert_eq!(tris.len(), 3);
        assert!(tris.contains_key(b"hel"));
        assert!(tris.contains_key(b"ell"));
        assert!(tris.contains_key(b"llo"));
    }

    #[test]
    fn test_extract_trigrams_short() {
        let data = b"ab";
        let tris = extract_trigrams(data);
        assert!(tris.is_empty());
    }

    #[test]
    fn test_is_likely_binary() {
        assert!(is_likely_binary(b"hello\x00world"));
        assert!(!is_likely_binary(b"hello world"));
    }

    #[test]
    fn test_count_substring_occurrences() {
        assert_eq!(count_substring_occurrences(b"abcabcabc", b"abc"), 3);
        assert_eq!(count_substring_occurrences(b"aaa", b"aa"), 1); // non-overlapping
        assert_eq!(count_substring_occurrences(b"hello", b"xyz"), 0);
        assert_eq!(count_substring_occurrences(b"", b"a"), 0);
    }

    #[test]
    fn test_build_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("foo.txt");
        let file2 = dir.path().join("bar.txt");
        let file3 = dir.path().join("baz.txt");

        std::fs::write(&file1, "the quick brown fox").unwrap();
        std::fs::write(&file2, "jumps over the lazy dog").unwrap();
        std::fs::write(&file3, "binary content is skipped").unwrap();

        let idx = NgramIndex::build(dir.path()).unwrap();
        assert!(idx.files.len() >= 3);

        // Search for "quick" should find foo.txt
        let results = idx.search("quick", 10);
        assert!(!results.is_empty());
        assert!(results[0].path.ends_with("foo.txt"));

        // Search for "lazy" should find bar.txt
        let results = idx.search("lazy", 10);
        assert!(!results.is_empty());
        assert!(results[0].path.ends_with("bar.txt"));

        // Search for "nonexistent_string_xyz" should find nothing
        let results = idx.search("nonexistent_string_xyz", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("test.txt");
        std::fs::write(&file1, "hello world trigram index test").unwrap();

        let idx = NgramIndex::build(dir.path()).unwrap();
        let save_path = dir.path().join("index.json");
        idx.save(&save_path).unwrap();

        let loaded = NgramIndex::load(&save_path).unwrap();
        assert_eq!(loaded.files.len(), idx.files.len());
        assert_eq!(loaded.index.len(), idx.index.len());

        // Loaded index should still find content
        let results = loaded.search("trigram", 10);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_incremental_update() {
        let dir = tempfile::tempdir().unwrap();
        let file1 = dir.path().join("a.txt");
        std::fs::write(&file1, "original content here").unwrap();

        let mut idx = NgramIndex::build(dir.path()).unwrap();

        // Searching for "original" should work
        let results = idx.search("original", 10);
        assert!(!results.is_empty());

        // Update file content
        std::fs::write(&file1, "modified content here").unwrap();
        idx.update(&[file1.clone()]).unwrap();

        // "original" should no longer match
        let results = idx.search("original", 10);
        assert!(results.is_empty());

        // "modified" should match
        let results = idx.search("modified", 10);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_stats() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("x.txt"), "some text for stats").unwrap();

        let idx = NgramIndex::build(dir.path()).unwrap();
        let stats = idx.stats();
        assert_eq!(stats.file_count, 1);
        assert!(stats.trigram_count > 0);
        assert!(stats.index_size_bytes > 0);
    }
}
