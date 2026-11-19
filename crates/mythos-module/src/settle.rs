//! When a change to the installed modules has finished happening.
//!
//! Installing a module is rarely one step. On Windows a running executable cannot be replaced, so
//! an installer renames the old build away and then moves the new one in: between the two, the
//! module is not there at all. A copy written in place grows for a while before it is complete.
//! Acting on every look would stop a connection because its module vanished for a moment, or
//! start a half-written file. So a change is acted on once it has held still for one look.

/// Tracks what is installed, and says when a change to it has settled.
#[derive(Debug, Clone)]
pub struct Settle<T> {
    /// What the last change acted on was, or what was there at the start.
    applied: T,
    /// What the previous look found.
    seen: T,
}

impl<T: PartialEq + Clone> Settle<T> {
    /// Start from what is installed now; that is not a change.
    pub fn new(now: T) -> Self {
        Self {
            applied: now.clone(),
            seen: now,
        }
    }

    /// Record another look. Returns what to act on when it differs from what was last acted on
    /// and the previous look found the same; otherwise `None`.
    pub fn observe(&mut self, now: T) -> Option<T> {
        let settled = now == self.seen && now != self.applied;
        self.seen = now.clone();
        if settled {
            self.applied = now.clone();
            Some(now)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_changed_is_never_a_change() {
        let mut settle = Settle::new("a");
        for _ in 0..5 {
            assert_eq!(settle.observe("a"), None);
        }
    }

    #[test]
    fn a_change_is_acted_on_once_it_has_held_for_one_look() {
        let mut settle = Settle::new("a");
        assert_eq!(
            settle.observe("b"),
            None,
            "first seen: may still be changing"
        );
        assert_eq!(settle.observe("b"), Some("b"), "held still: act");
        assert_eq!(settle.observe("b"), None, "already acted on");
    }

    #[test]
    fn a_module_missing_for_one_look_is_not_a_change() {
        // Windows: old build renamed away, new one not yet moved in, then the same file back.
        let mut settle = Settle::new("a");
        assert_eq!(settle.observe("missing"), None);
        assert_eq!(settle.observe("a"), None);
        assert_eq!(settle.observe("a"), None);
    }

    #[test]
    fn a_file_still_being_written_is_acted_on_when_it_stops_changing() {
        let mut settle = Settle::new(0);
        assert_eq!(settle.observe(10), None);
        assert_eq!(settle.observe(20), None);
        assert_eq!(settle.observe(30), None);
        assert_eq!(settle.observe(30), Some(30));
    }

    #[test]
    fn a_change_that_reverts_before_it_settles_is_never_acted_on() {
        let mut settle = Settle::new("a");
        assert_eq!(settle.observe("b"), None);
        assert_eq!(settle.observe("a"), None);
        assert_eq!(settle.observe("a"), None);
    }
}
