#![forbid(unsafe_code)]

//! Grapheme pooling and interning.
//!
//! The `GraphemePool` stores complex grapheme clusters (emoji, ZWJ sequences, etc.)
//! that don't fit in `CellContent`'s 4-byte inline storage. It provides:
//!
//! - Compact `GraphemeId` references (4 bytes) instead of heap strings per cell
//! - Reference counting for automatic cleanup
//! - Deduplication via hash lookup
//! - Slot reuse via free list
//!
//! # When to Use
//!
//! Most cells use simple characters that fit inline in `CellContent`. The pool
//! is only needed for:
//! - Multi-codepoint emoji (👨‍👩‍👧‍👦, 🧑🏽‍💻, etc.)
//! - ZWJ sequences
//! - Complex combining character sequences
//!
//! # Usage
//!
//! ```
//! use ftui_render::grapheme_pool::GraphemePool;
//!
//! let mut pool = GraphemePool::new();
//!
//! // Intern a grapheme
//! let id = pool.intern("👨‍👩‍👧‍👦", 2); // Family emoji, width 2
//!
//! // Look it up
//! assert_eq!(pool.get(id), Some("👨‍👩‍👧‍👦"));
//! assert_eq!(id.width(), 2);
//!
//! // Increment reference count when copied to another cell
//! pool.retain(id);
//!
//! // Release when cell is overwritten
//! pool.release(id);
//! pool.release(id);
//!
//! // After all references released, slot is freed
//! assert_eq!(pool.get(id), None);
//! ```

use crate::cell::GraphemeId;
use std::collections::HashMap;

/// A slot in the grapheme pool.
#[derive(Debug, Clone)]
struct GraphemeSlot {
    /// The grapheme cluster string.
    text: String,
    /// Display width (cached from GraphemeId).
    /// Note: Width is also embedded in GraphemeId, but kept here for debugging.
    #[allow(dead_code)]
    width: u8,
    /// Reference count.
    refcount: u32,
}

/// A reference-counted pool for complex grapheme clusters.
///
/// Stores multi-codepoint strings and returns compact `GraphemeId` references.
#[derive(Debug, Clone)]
pub struct GraphemePool {
    /// Slot storage. `None` indicates a free slot.
    slots: Vec<Option<GraphemeSlot>>,
    /// Lookup table for deduplication.
    lookup: HashMap<String, GraphemeId>,
    /// Free slot indices for reuse.
    free_list: Vec<u32>,
}

impl GraphemePool {
    /// Create a new empty grapheme pool.
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            lookup: HashMap::new(),
            free_list: Vec::new(),
        }
    }

    /// Create a pool with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            lookup: HashMap::with_capacity(capacity),
            free_list: Vec::new(),
        }
    }

    /// Number of active (non-free) slots.
    pub fn len(&self) -> usize {
        self.slots.len() - self.free_list.len()
    }

    /// Check if the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total capacity (including free slots).
    pub fn capacity(&self) -> usize {
        self.slots.capacity()
    }

    /// Intern a grapheme string and return its ID.
    ///
    /// If the string is already interned, returns the existing ID and
    /// increments the reference count.
    ///
    /// # Parameters
    ///
    /// - `text`: The grapheme cluster string
    /// - `width`: Display width (0-127)
    ///
    /// # Panics
    ///
    /// Panics if width > 127 or if the pool exceeds capacity (16M slots).
    pub fn intern(&mut self, text: &str, width: u8) -> GraphemeId {
        assert!(width <= GraphemeId::MAX_WIDTH, "width overflow");

        // Check if already interned
        if let Some(&id) = self.lookup.get(text) {
            self.retain(id);
            return id;
        }

        // Allocate a new slot
        let slot_idx = self.alloc_slot();
        let id = GraphemeId::new(slot_idx, width);

        // Store the grapheme
        let slot = GraphemeSlot {
            text: text.to_string(),
            width,
            refcount: 1,
        };

        if (slot_idx as usize) < self.slots.len() {
            self.slots[slot_idx as usize] = Some(slot);
        } else {
            debug_assert_eq!(slot_idx as usize, self.slots.len());
            self.slots.push(Some(slot));
        }

        self.lookup.insert(text.to_string(), id);
        id
    }

    /// Get the string for a grapheme ID.
    ///
    /// Returns `None` if the ID is invalid or has been freed.
    pub fn get(&self, id: GraphemeId) -> Option<&str> {
        self.slots
            .get(id.slot())
            .and_then(|slot| slot.as_ref())
            .map(|slot| slot.text.as_str())
    }

    /// Increment the reference count for a grapheme.
    ///
    /// Call this when a cell containing this grapheme is copied.
    pub fn retain(&mut self, id: GraphemeId) {
        if let Some(Some(slot)) = self.slots.get_mut(id.slot()) {
            slot.refcount = slot.refcount.saturating_add(1);
        }
    }

    /// Decrement the reference count for a grapheme.
    ///
    /// Call this when a cell containing this grapheme is overwritten or freed.
    /// When the reference count reaches zero, the slot is freed for reuse.
    pub fn release(&mut self, id: GraphemeId) {
        let slot_idx = id.slot();
        if let Some(Some(slot)) = self.slots.get_mut(slot_idx) {
            slot.refcount = slot.refcount.saturating_sub(1);
            if slot.refcount == 0 {
                // Remove from lookup
                self.lookup.remove(&slot.text);
                // Clear the slot
                self.slots[slot_idx] = None;
                // Add to free list
                self.free_list.push(slot_idx as u32);
            }
        }
    }

    /// Get the reference count for a grapheme.
    ///
    /// Returns 0 if the ID is invalid or freed.
    pub fn refcount(&self, id: GraphemeId) -> u32 {
        self.slots
            .get(id.slot())
            .and_then(|slot| slot.as_ref())
            .map(|slot| slot.refcount)
            .unwrap_or(0)
    }

    /// Clear all entries from the pool.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.lookup.clear();
        self.free_list.clear();
    }

    /// Allocate a slot index, reusing from free list if possible.
    fn alloc_slot(&mut self) -> u32 {
        if let Some(idx) = self.free_list.pop() {
            idx
        } else {
            let idx = self.slots.len() as u32;
            assert!(
                idx <= GraphemeId::MAX_SLOT,
                "grapheme pool capacity exceeded"
            );
            idx
        }
    }
}

impl Default for GraphemePool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_and_get() {
        let mut pool = GraphemePool::new();
        let id = pool.intern("👨‍👩‍👧‍👦", 2);

        assert_eq!(pool.get(id), Some("👨‍👩‍👧‍👦"));
        assert_eq!(id.width(), 2);
    }

    #[test]
    fn deduplication() {
        let mut pool = GraphemePool::new();
        let id1 = pool.intern("🎉", 2);
        let id2 = pool.intern("🎉", 2);

        // Same ID returned
        assert_eq!(id1, id2);
        // Refcount is 2
        assert_eq!(pool.refcount(id1), 2);
        // Only one slot used
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn retain_and_release() {
        let mut pool = GraphemePool::new();
        let id = pool.intern("🚀", 2);
        assert_eq!(pool.refcount(id), 1);

        pool.retain(id);
        assert_eq!(pool.refcount(id), 2);

        pool.release(id);
        assert_eq!(pool.refcount(id), 1);

        pool.release(id);
        // Slot is now freed
        assert_eq!(pool.get(id), None);
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn slot_reuse() {
        let mut pool = GraphemePool::new();

        // Intern and release
        let id1 = pool.intern("A", 1);
        pool.release(id1);
        assert_eq!(pool.len(), 0);

        // Intern again - should reuse the slot
        let id2 = pool.intern("B", 1);
        assert_eq!(id1.slot(), id2.slot());
        assert_eq!(pool.get(id2), Some("B"));
    }

    #[test]
    fn empty_pool() {
        let pool = GraphemePool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn multiple_graphemes() {
        let mut pool = GraphemePool::new();

        let id1 = pool.intern("👨‍💻", 2);
        let id2 = pool.intern("👩‍🔬", 2);
        let id3 = pool.intern("🧑🏽‍🚀", 2);

        assert_eq!(pool.len(), 3);
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);

        assert_eq!(pool.get(id1), Some("👨‍💻"));
        assert_eq!(pool.get(id2), Some("👩‍🔬"));
        assert_eq!(pool.get(id3), Some("🧑🏽‍🚀"));
    }

    #[test]
    fn width_preserved() {
        let mut pool = GraphemePool::new();

        // Various widths
        let id1 = pool.intern("👋", 2);
        let id2 = pool.intern("A", 1);
        let id3 = pool.intern("日", 2);

        assert_eq!(id1.width(), 2);
        assert_eq!(id2.width(), 1);
        assert_eq!(id3.width(), 2);
    }

    #[test]
    fn clear_pool() {
        let mut pool = GraphemePool::new();
        pool.intern("A", 1);
        pool.intern("B", 1);
        pool.intern("C", 1);

        assert_eq!(pool.len(), 3);

        pool.clear();
        assert!(pool.is_empty());
    }

    #[test]
    fn invalid_id_returns_none() {
        let pool = GraphemePool::new();
        let fake_id = GraphemeId::new(999, 1);
        assert_eq!(pool.get(fake_id), None);
    }

    #[test]
    fn release_invalid_id_is_safe() {
        let mut pool = GraphemePool::new();
        let fake_id = GraphemeId::new(999, 1);
        pool.release(fake_id); // Should not panic
    }

    #[test]
    fn retain_invalid_id_is_safe() {
        let mut pool = GraphemePool::new();
        let fake_id = GraphemeId::new(999, 1);
        pool.retain(fake_id); // Should not panic
    }

    #[test]
    #[should_panic(expected = "width overflow")]
    fn width_overflow_panics() {
        let mut pool = GraphemePool::new();
        pool.intern("X", 128); // Max is 127
    }

    #[test]
    fn with_capacity() {
        let pool = GraphemePool::with_capacity(100);
        assert!(pool.capacity() >= 100);
        assert!(pool.is_empty());
    }
}
