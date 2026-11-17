//! What an agent reads from `memory_search` and `skill_list`: the best items
//! first, one page at a time, so a search costs a few lines and not the whole
//! memory (docs/decisions.md §33).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::memory::{Note, is_stale};

/// Notes `memory_search` returns by default, and at most.
pub const NOTES_PAGE: usize = 8;
pub const NOTES_MAX: usize = 30;
/// Skills `skill_list` returns by default, and at most.
pub const SKILLS_PAGE: usize = 20;
pub const SKILLS_MAX: usize = 50;

/// The page an agent asked for, as it sent it: missing or nonsense values
/// fall back to the defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRequest {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// One page of a longer list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paged<T> {
    pub items: Vec<T>,
    /// Where the page starts.
    pub offset: usize,
    /// How many come after it.
    pub remaining: usize,
}

impl<T> Paged<T> {
    /// The offset that shows the next page.
    pub fn next_offset(&self) -> usize {
        self.offset + self.items.len()
    }
}

/// Cuts `items` to the page `request` asks for: `default` items unless it
/// gives a limit, never more than `max`, from its offset (0 by default).
pub fn paginate<T>(items: Vec<T>, request: PageRequest, default: usize, max: usize) -> Paged<T> {
    let limit = match request.limit {
        Some(n) if n >= 1 => (n as u64).min(max as u64) as usize,
        _ => default.min(max),
    };
    let offset = request.offset.unwrap_or(0).max(0) as u64;
    let offset = offset.min(items.len() as u64) as usize;
    let total = items.len();
    let items: Vec<T> = items.into_iter().skip(offset).take(limit).collect();
    let remaining = total - offset - items.len();
    Paged {
        items,
        offset,
        remaining,
    }
}

/// How the tasks of one project used a note.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoteUse {
    /// Distinct tasks that read it.
    pub tasks: u32,
    /// Seconds.
    pub last_used: i64,
}

/// A use this recent counts as recent.
pub const RECENT_USE_SECS: i64 = 14 * 24 * 60 * 60;
/// Places a note may climb for being read by other tasks (half a place per
/// task, up to this), plus [`RECENT_BOOST`] when one read it recently.
const USE_BOOST_MAX: f64 = 2.0;
const USE_BOOST_PER_TASK: f64 = 0.5;
const RECENT_BOOST: f64 = 1.0;

/// Orders the notes an agent gets for a search. `candidates` come best match
/// first (full-text rank; newest first for an empty query):
///
/// 1. the search decides: each note starts at its place in `candidates`;
/// 2. notes read by this project's tasks climb a little, at most three
///    places (two for being used often, one for being used recently), so a
///    used note wins a near tie but never beats a much better match;
/// 3. notes "para revisar" ([`is_stale`]: approved but unused for 60 days)
///    go after every other note, so they only show when nothing better
///    matches.
///
/// Equal scores keep the search's order.
pub fn rank_notes(candidates: Vec<Note>, uses: &HashMap<i64, NoteUse>, now: i64) -> Vec<Note> {
    let mut scored: Vec<(bool, f64, Note)> = candidates
        .into_iter()
        .enumerate()
        .map(|(place, note)| {
            let boost = uses.get(&note.id).map_or(0.0, |u| {
                let often = (f64::from(u.tasks) * USE_BOOST_PER_TASK).min(USE_BOOST_MAX);
                let recent = if u.tasks > 0 && now - u.last_used <= RECENT_USE_SECS {
                    RECENT_BOOST
                } else {
                    0.0
                };
                often + recent
            });
            (is_stale(&note, now), place as f64 - boost, note)
        })
        .collect();
    // Stable: equal scores keep the search's order.
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    scored.into_iter().map(|(_, _, note)| note).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Category, NoteStatus};

    const DAY: i64 = 24 * 60 * 60;

    fn note(id: i64, last_used: Option<i64>) -> Note {
        Note {
            id,
            project: None,
            text: format!("nota {id}"),
            category: Category::Decision,
            author: "user".into(),
            status: NoteStatus::Approved,
            created_at: 100 * DAY,
            uses: 0,
            last_used,
            topic: None,
            updated_at: None,
        }
    }

    fn ids(notes: &[Note]) -> Vec<i64> {
        notes.iter().map(|n| n.id).collect()
    }

    #[test]
    fn pages_default_clamp_and_count_the_rest() {
        let items: Vec<u32> = (0..20).collect();
        let first = paginate(items.clone(), PageRequest::default(), 8, 30);
        assert_eq!(first.items, (0..8).collect::<Vec<_>>());
        assert_eq!(
            (first.offset, first.remaining, first.next_offset()),
            (0, 12, 8)
        );

        let second = paginate(
            items.clone(),
            PageRequest {
                limit: None,
                offset: Some(8),
            },
            8,
            30,
        );
        assert_eq!(second.items[0], 8);
        assert_eq!(second.remaining, 4);

        let all = paginate(
            items.clone(),
            PageRequest {
                limit: Some(500),
                offset: Some(-3),
            },
            8,
            30,
        );
        assert_eq!((all.items.len(), all.offset, all.remaining), (20, 0, 0));

        let past_the_end = paginate(
            items.clone(),
            PageRequest {
                limit: Some(0),
                offset: Some(99),
            },
            8,
            30,
        );
        assert!(past_the_end.items.is_empty());
        assert_eq!((past_the_end.offset, past_the_end.remaining), (20, 0));

        let capped = paginate(
            (0..100).collect::<Vec<u32>>(),
            PageRequest {
                limit: Some(40),
                offset: None,
            },
            8,
            30,
        );
        assert_eq!((capped.items.len(), capped.remaining), (30, 70));
    }

    #[test]
    fn the_search_decides_and_use_breaks_near_ties() {
        let now = 120 * DAY;
        let candidates: Vec<Note> = (1..=6).map(|id| note(id, Some(now - DAY))).collect();
        // No uses: the search's order.
        assert_eq!(
            ids(&rank_notes(candidates.clone(), &HashMap::new(), now)),
            [1, 2, 3, 4, 5, 6]
        );
        // Note 3, read by four tasks of the project yesterday, climbs three
        // places at most: it passes 2 and 1 (a near tie) but not further.
        let uses = HashMap::from([(
            3,
            NoteUse {
                tasks: 4,
                last_used: now - DAY,
            },
        )]);
        assert_eq!(
            ids(&rank_notes(candidates.clone(), &uses, now)),
            [3, 1, 2, 4, 5, 6]
        );
        // Note 6, read once long ago: half a place, not enough to pass 5.
        let old = HashMap::from([(
            6,
            NoteUse {
                tasks: 1,
                last_used: now - 30 * DAY,
            },
        )]);
        assert_eq!(ids(&rank_notes(candidates, &old, now)), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn notes_to_review_go_last() {
        let now = 200 * DAY;
        // Note 1 is the best match but unused for over 60 days.
        let candidates = vec![
            note(1, Some(now - 90 * DAY)),
            note(2, Some(now - DAY)),
            note(3, None),
            note(4, Some(now - 2 * DAY)),
        ];
        // Note 3 never used, created on day 100: stale too.
        let ranked = rank_notes(candidates, &HashMap::new(), now);
        assert_eq!(ids(&ranked), [2, 4, 1, 3]);
        // A stale note alone still shows: nothing better matched.
        let alone = rank_notes(vec![note(1, Some(now - 90 * DAY))], &HashMap::new(), now);
        assert_eq!(ids(&alone), [1]);
    }
}
