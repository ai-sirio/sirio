//! Commit-graph lane layout: which column each commit's node sits in, and
//! which lines connect the rows.

use crate::CommitRecord;

/// How many distinct lane colours the palette offers. The renderer maps this
/// index onto real colours; the layout only cycles indices.
pub const LANE_COLORS: usize = 6;

/// One rendered row of the graph column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRow {
    /// Column of this commit's node.
    pub lane: usize,
    /// Palette index of the node and of the lane it continues on.
    pub color: usize,
    /// Per column, the colour of the lane that continues *below* this row.
    /// `None` is a hole: a column whose lane has ended and which is free for
    /// reuse. Holes are never compacted away — compacting would slide live
    /// branches sideways on every commit, which reads as the graph dancing
    /// during a scroll.
    pub through: Vec<Option<usize>>,
    /// `(source column, colour)` for each lane that ends on this commit and
    /// must be drawn folding into its node.
    pub joins_in: Vec<(usize, usize)>,
    /// `(target column, colour)` for each parent beyond the first, which
    /// leaves this node for a lane of its own.
    pub edges_out: Vec<(usize, usize)>,
}

/// One live lane: the commit it is waiting for, and the colour it draws in.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Lane {
    expects: String,
    color: usize,
}

/// Opaque continuation state for an incremental graph layout.
///
/// The lane representation is deliberately private: callers can retain and
/// replace the cursor, but cannot construct or modify graph state themselves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutCursor {
    lanes: Vec<Option<Lane>>,
    next_color: usize,
}

/// Rows produced by one layout page and the state needed to continue it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutChunk {
    pub rows: Vec<GraphRow>,
    cursor: LayoutCursor,
}

impl LayoutChunk {
    /// Returns the opaque state for laying out the next page.
    pub fn cursor(&self) -> &LayoutCursor {
        &self.cursor
    }
}

/// Assigns every commit a column and the edges around it.
///
/// The commits must be in the order `git log --date-order` returned them
/// (newest first). Each commit takes the lane already expecting its sha, or
/// the leftmost free column; its first parent inherits that lane, and every
/// further parent takes the lane already expecting it or opens a new one.
pub fn layout(commits: &[CommitRecord]) -> LayoutChunk {
    extend_layout(
        &LayoutCursor {
            lanes: Vec::new(),
            next_color: 0,
        },
        commits,
    )
}

/// Continues a graph layout from a previously returned cursor.
pub fn extend_layout(cursor: &LayoutCursor, commits: &[CommitRecord]) -> LayoutChunk {
    let mut lanes = cursor.lanes.clone();
    let mut next_color = cursor.next_color;
    let mut rows = Vec::with_capacity(commits.len());

    for commit in commits {
        // Every lane waiting for this commit converges here. The leftmost is
        // the one the commit is drawn on; the rest fold in and end.
        let waiting: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, lane)| lane.as_ref().is_some_and(|lane| lane.expects == commit.sha))
            .map(|(column, _)| column)
            .collect();

        let (lane_column, color) = match waiting.first() {
            Some(&column) => {
                let color = lanes[column]
                    .as_ref()
                    .expect("a waiting column holds a lane")
                    .color;
                (column, color)
            }
            None => {
                let color = pick_color(&lanes, &mut next_color);
                let column = free_column(&mut lanes);
                (column, color)
            }
        };

        let joins_in = waiting
            .iter()
            .skip(1)
            .map(|&column| {
                let color = lanes[column]
                    .as_ref()
                    .expect("a waiting column holds a lane")
                    .color;
                (column, color)
            })
            .collect::<Vec<_>>();
        for &column in waiting.iter().skip(1) {
            lanes[column] = None;
        }

        // The first parent inherits this lane; the lane ends when there is
        // none (a root commit).
        match commit.parents.first() {
            Some(parent) => {
                lanes[lane_column] = Some(Lane {
                    expects: parent.clone(),
                    color,
                });
            }
            None => lanes[lane_column] = None,
        }

        let mut edges_out = Vec::new();
        for parent in commit.parents.iter().skip(1) {
            if let Some(column) = lanes
                .iter()
                .position(|lane| lane.as_ref().is_some_and(|lane| &lane.expects == parent))
            {
                let color = lanes[column]
                    .as_ref()
                    .expect("a matched column holds a lane")
                    .color;
                edges_out.push((column, color));
                continue;
            }
            let color = pick_color(&lanes, &mut next_color);
            let column = free_column(&mut lanes);
            lanes[column] = Some(Lane {
                expects: parent.clone(),
                color,
            });
            edges_out.push((column, color));
        }

        let through = lanes
            .iter()
            .map(|lane| lane.as_ref().map(|lane| lane.color))
            .collect();

        rows.push(GraphRow {
            lane: lane_column,
            color,
            through,
            joins_in,
            edges_out,
        });
    }

    LayoutChunk {
        rows,
        cursor: LayoutCursor { lanes, next_color },
    }
}

/// The leftmost hole, or a new column at the right edge. Reserved by leaving
/// the slot `None`; the caller fills it.
fn free_column(lanes: &mut Vec<Option<Lane>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(column) => column,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

/// The next palette index that no live lane is already using. Two adjacent
/// lanes sharing a colour read as one branch, which is the single most
/// common way a commit graph misleads.
fn pick_color(lanes: &[Option<Lane>], next_color: &mut usize) -> usize {
    let live: Vec<usize> = lanes
        .iter()
        .filter_map(|lane| lane.as_ref().map(|lane| lane.color))
        .collect();
    for _ in 0..LANE_COLORS {
        let candidate = *next_color % LANE_COLORS;
        *next_color = next_color.wrapping_add(1);
        if !live.contains(&candidate) {
            return candidate;
        }
    }
    *next_color % LANE_COLORS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommitRecord;

    fn commit(sha: &str, parents: &[&str]) -> CommitRecord {
        CommitRecord {
            sha: sha.to_owned(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            ..CommitRecord::default()
        }
    }

    #[test]
    fn a_linear_history_uses_one_lane() {
        let commits = [
            commit("c3", &["c2"]),
            commit("c2", &["c1"]),
            commit("c1", &[]),
        ];

        let rows = layout(&commits).rows;

        assert!(rows.iter().all(|row| row.lane == 0));
        assert!(rows.iter().all(|row| row.joins_in.is_empty()));
    }

    #[test]
    fn the_last_row_of_a_linear_history_continues_nothing() {
        let commits = [commit("c2", &["c1"]), commit("c1", &[])];

        let rows = layout(&commits).rows;

        assert_eq!(rows[0].through, vec![Some(rows[0].color)]);
        assert!(rows[1].through.iter().all(Option::is_none));
    }

    #[test]
    fn a_second_parent_opens_a_new_lane() {
        // m -> (main, side); main -> base; side -> base; base is root.
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
        ];

        let rows = layout(&commits).rows;

        assert_eq!(rows[0].lane, 0, "the merge sits on the lane it inherited");
        assert_eq!(
            rows[0].edges_out.len(),
            1,
            "the second parent leaves for its own lane"
        );
        assert_eq!(rows[0].edges_out[0].0, 1, "leftmost free column");
        assert_eq!(rows[2].lane, 1, "`side` is drawn on the lane opened for it");
    }

    #[test]
    fn converging_lanes_join_on_the_commit_they_share() {
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
        ];

        let rows = layout(&commits).rows;

        let base = &rows[3];
        assert_eq!(base.lane, 0);
        assert_eq!(base.joins_in.len(), 1, "the side lane folds into base");
        assert_eq!(base.joins_in[0].0, 1, "it comes from column 1");
    }

    #[test]
    fn a_disconnected_history_starts_in_the_leftmost_column() {
        // All earlier lanes have ended; a disconnected root starts at column
        // 0 rather than inheriting the width of unrelated history.
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
            commit("orphan", &[]),
        ];

        let rows = layout(&commits).rows;

        assert_eq!(rows[4].lane, 0);
    }

    #[test]
    fn a_closed_lane_reuses_its_hole_while_another_lane_stays_live() {
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &[]),
            commit("unrelated", &[]),
            commit("base", &[]),
        ];

        let rows = layout(&commits).rows;

        assert_eq!(rows[2].lane, 1, "the side lane closes in column 1");
        assert_eq!(rows[3].lane, 1, "the hole is reused while main stays live");
    }

    #[test]
    fn a_new_lane_avoids_the_colours_of_live_lanes() {
        let commits = [
            commit("m", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
        ];

        let rows = layout(&commits).rows;

        let opened_colour = rows[0].edges_out[0].1;
        assert_ne!(
            opened_colour, rows[0].color,
            "two live lanes must not share a colour"
        );
    }

    #[test]
    fn an_empty_history_lays_out_to_nothing() {
        assert!(layout(&[]).rows.is_empty());
    }

    fn assert_every_partition_matches_full_layout(commits: &[CommitRecord]) {
        let expected = layout(commits).rows;
        for split in 0..=commits.len() {
            let first = layout(&commits[..split]);
            let rest = extend_layout(first.cursor(), &commits[split..]);
            let mut actual = first.rows;
            actual.extend(rest.rows);
            assert_eq!(actual, expected, "partition at {split}");
        }
    }

    #[test]
    fn a_linear_history_continues_across_pages() {
        let commits = [
            commit("c4", &["c3"]),
            commit("c3", &["c2"]),
            commit("c2", &["c1"]),
            commit("c1", &[]),
        ];

        assert_every_partition_matches_full_layout(&commits);
    }

    #[test]
    fn a_merge_split_across_pages_preserves_the_open_branch() {
        let commits = [
            commit("merge", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &["base"]),
            commit("base", &[]),
        ];

        assert_every_partition_matches_full_layout(&commits);
    }

    #[test]
    fn a_branch_convergence_split_across_pages_preserves_the_join() {
        let commits = [
            commit("tip", &["left", "right"]),
            commit("left", &["base"]),
            commit("right", &["base"]),
            commit("base", &[]),
        ];
        let first = layout(&commits[..3]);
        let rest = extend_layout(first.cursor(), &commits[3..]);

        assert_eq!(
            rest.rows[0].joins_in,
            vec![(1, first.rows[0].edges_out[0].1)]
        );
        assert_eq!(
            first.rows.into_iter().chain(rest.rows).collect::<Vec<_>>(),
            layout(&commits).rows
        );
    }

    #[test]
    fn a_reused_hole_and_palette_continue_across_pages() {
        let commits = [
            commit("merge", &["main", "side"]),
            commit("main", &["base"]),
            commit("side", &[]),
            commit("unrelated", &[]),
            commit("base", &[]),
        ];
        let first = layout(&commits[..3]);
        let rest = extend_layout(first.cursor(), &commits[3..]);

        assert_eq!(rest.rows[0].lane, 1);
        assert_eq!(rest.rows[0].color, layout(&commits).rows[3].color);
        assert_eq!(
            first.rows.into_iter().chain(rest.rows).collect::<Vec<_>>(),
            layout(&commits).rows
        );
    }

    #[test]
    fn an_empty_layout_cursor_is_a_valid_fresh_reset() {
        let commits = [commit("tip", &["base"]), commit("base", &[])];
        let fresh = layout(&[]);
        let continued = extend_layout(fresh.cursor(), &commits);

        assert_eq!(continued.rows, layout(&commits).rows);
    }

    #[test]
    fn every_representative_partition_is_strictly_equivalent() {
        let fixtures = [
            vec![
                commit("c3", &["c2"]),
                commit("c2", &["c1"]),
                commit("c1", &[]),
            ],
            vec![
                commit("merge", &["main", "side"]),
                commit("main", &["base"]),
                commit("side", &["base"]),
                commit("base", &[]),
            ],
            vec![
                commit("merge", &["main", "side"]),
                commit("main", &["base"]),
                commit("side", &[]),
                commit("unrelated", &[]),
                commit("base", &[]),
            ],
        ];

        for fixture in fixtures {
            assert_every_partition_matches_full_layout(&fixture);
        }
    }
}
