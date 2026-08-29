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
#[derive(Clone, Debug)]
struct Lane {
    expects: String,
    color: usize,
}

/// Assigns every commit a column and the edges around it.
///
/// The commits must be in the order `git log --date-order` returned them
/// (newest first). Each commit takes the lane already expecting its sha, or
/// the leftmost free column; its first parent inherits that lane, and every
/// further parent takes the lane already expecting it or opens a new one.
pub fn layout(commits: &[CommitRecord]) -> Vec<GraphRow> {
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut next_color = 0usize;
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

    rows
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

        let rows = layout(&commits);

        assert!(rows.iter().all(|row| row.lane == 0));
        assert!(rows.iter().all(|row| row.joins_in.is_empty()));
    }

    #[test]
    fn the_last_row_of_a_linear_history_continues_nothing() {
        let commits = [commit("c2", &["c1"]), commit("c1", &[])];

        let rows = layout(&commits);

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

        let rows = layout(&commits);

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

        let rows = layout(&commits);

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

        let rows = layout(&commits);

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

        let rows = layout(&commits);

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

        let rows = layout(&commits);

        let opened_colour = rows[0].edges_out[0].1;
        assert_ne!(
            opened_colour, rows[0].color,
            "two live lanes must not share a colour"
        );
    }

    #[test]
    fn an_empty_history_lays_out_to_nothing() {
        assert!(layout(&[]).is_empty());
    }
}
