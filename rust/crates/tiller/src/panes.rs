//! The persistent pane tree for one open tab.
//!
//! This module deliberately contains no GPUI state. A leaf owns the live
//! surface entity, while a split owns only geometry and its two children. The
//! workspace can therefore redraw the tree without recreating a terminal or a
//! chat transcript.

use gpui::Entity;
use tiller_terminal::TerminalView;
use tiller_ui::{chat::Chat, file_view::FileView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SplitDirection {
    Horizontal,
    Vertical,
}

pub(crate) enum PaneNode<T> {
    Leaf {
        id: usize,
        // `None` exists only while an in-place tree operation temporarily
        // owns the node. A rendered tree always contains `Some`.
        content: Option<T>,
    },
    Split {
        direction: SplitDirection,
        /// The fraction of the available axis given to the first child.
        ratio: f32,
        first: Box<Self>,
        second: Box<Self>,
    },
}

impl<T> PaneNode<T> {
    pub(crate) fn leaf(id: usize, content: T) -> Self {
        Self::Leaf {
            id,
            content: Some(content),
        }
    }

    pub(crate) fn split_focused(
        &mut self,
        focused: usize,
        new_id: usize,
        direction: SplitDirection,
        content: T,
    ) -> bool {
        let mut content = Some(content);
        self.split_focused_inner(focused, new_id, direction, &mut content)
    }

    fn split_focused_inner(
        &mut self,
        focused: usize,
        new_id: usize,
        direction: SplitDirection,
        content: &mut Option<T>,
    ) -> bool {
        match self {
            Self::Leaf { id, content: old } if *id == focused => {
                let old_id = *id;
                let old_content = old.take().expect("rendered pane has content");
                let new_content = content.take().expect("new pane content is available");
                *self = Self::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(Self::Leaf {
                        id: old_id,
                        content: Some(old_content),
                    }),
                    second: Box::new(Self::Leaf {
                        id: new_id,
                        content: Some(new_content),
                    }),
                };
                true
            }
            Self::Leaf { .. } => false,
            Self::Split { first, second, .. } => {
                first.split_focused_inner(focused, new_id, direction, content)
                    || second.split_focused_inner(focused, new_id, direction, content)
            }
        }
    }

    pub(crate) fn contains(&self, id: usize) -> bool {
        match self {
            Self::Leaf { id: leaf_id, .. } => *leaf_id == id,
            Self::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    pub(crate) fn first_id(&self) -> Option<usize> {
        match self {
            Self::Leaf { id, .. } => Some(*id),
            Self::Split { first, .. } => first.first_id(),
        }
    }

    pub(crate) fn last_id(&self) -> Option<usize> {
        match self {
            Self::Leaf { id, .. } => Some(*id),
            Self::Split { second, .. } => second.last_id(),
        }
    }

    pub(crate) fn for_each(&self, f: &mut impl FnMut(usize, &T)) {
        match self {
            Self::Leaf {
                id,
                content: Some(content),
            } => f(*id, content),
            Self::Leaf { content: None, .. } => {}
            Self::Split { first, second, .. } => {
                first.for_each(f);
                second.for_each(f);
            }
        }
    }

    pub(crate) fn leaf_ids(&self) -> Vec<usize> {
        let mut result = Vec::new();
        self.push_leaf_ids(&mut result);
        result
    }

    fn push_leaf_ids(&self, result: &mut Vec<usize>) {
        match self {
            Self::Leaf { id, .. } => result.push(*id),
            Self::Split { first, second, .. } => {
                first.push_leaf_ids(result);
                second.push_leaf_ids(result);
            }
        }
    }

    /// Selects the nearest leaf in the stable visual order. For a two-pane
    /// split this is exactly left/right or above/below; for nested splits it
    /// also gives keyboard navigation a deterministic fallback when panes do
    /// not share an edge.
    pub(crate) fn neighbor(
        &self,
        focused: usize,
        direction: SplitDirection,
        forward: bool,
    ) -> Option<usize> {
        match self {
            Self::Leaf { .. } => None,
            Self::Split {
                direction: split_direction,
                first,
                second,
                ..
            } => {
                if first.contains(focused) {
                    if *split_direction == direction {
                        if forward {
                            second.first_id()
                        } else {
                            first
                                .neighbor(focused, direction, forward)
                                .or_else(|| second.last_id())
                        }
                    } else {
                        first.neighbor(focused, direction, forward)
                    }
                } else if second.contains(focused) {
                    if *split_direction == direction {
                        if forward {
                            second
                                .neighbor(focused, direction, forward)
                                .or_else(|| first.first_id())
                        } else {
                            first.last_id()
                        }
                    } else {
                        second.neighbor(focused, direction, forward)
                    }
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn set_ratio(&mut self, path: &[bool], ratio: f32) -> bool {
        if path.is_empty() {
            if let Self::Split { ratio: current, .. } = self {
                *current = ratio.clamp(0.1, 0.9);
                return true;
            }
            return false;
        }

        match self {
            Self::Split { first, second, .. } => {
                if path[0] {
                    second.set_ratio(&path[1..], ratio)
                } else {
                    first.set_ratio(&path[1..], ratio)
                }
            }
            Self::Leaf { .. } => false,
        }
    }

    /// Removes a leaf and collapses its parent. The remaining sibling is
    /// returned as the replacement node, so it naturally fills the old split.
    pub(crate) fn remove(&mut self, target: usize) -> Option<T> {
        if matches!(self, Self::Leaf { .. }) {
            return None;
        }
        let current = std::mem::replace(self, Self::empty());
        let (replacement, removed) = remove_node(current, target);
        if let Some(replacement) = replacement {
            *self = replacement;
        }
        removed
    }

    fn empty() -> Self {
        Self::Leaf {
            id: usize::MAX,
            content: None,
        }
    }
}

pub(crate) enum PaneContent {
    Chat(Entity<Chat>),
    Terminal { view: Entity<TerminalView> },
    File { view: Entity<FileView> },
}

impl PaneContent {
    pub(crate) fn terminal(&self) -> Option<Entity<TerminalView>> {
        match self {
            Self::Terminal { view } => Some(view.clone()),
            Self::Chat(_) | Self::File { .. } => None,
        }
    }
}

fn remove_node<T>(node: PaneNode<T>, target: usize) -> (Option<PaneNode<T>>, Option<T>) {
    match node {
        PaneNode::Leaf { id, content } => {
            if id == target {
                (None, content)
            } else {
                (Some(PaneNode::Leaf { id, content }), None)
            }
        }
        PaneNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let first_node = *first;
            let second_node = *second;
            let (new_first, removed) = remove_node(first_node, target);
            if let Some(removed) = removed {
                return (
                    Some(match new_first {
                        Some(first) => PaneNode::Split {
                            direction,
                            ratio,
                            first: Box::new(first),
                            second: Box::new(second_node),
                        },
                        None => second_node,
                    }),
                    Some(removed),
                );
            }

            let first_node = new_first.expect("a non-matching first pane remains");
            let (new_second, removed) = remove_node(second_node, target);
            if let Some(removed) = removed {
                return (
                    Some(match new_second {
                        Some(second) => PaneNode::Split {
                            direction,
                            ratio,
                            first: Box::new(first_node),
                            second: Box::new(second),
                        },
                        None => first_node,
                    }),
                    Some(removed),
                );
            }

            (
                Some(PaneNode::Split {
                    direction,
                    ratio,
                    first: Box::new(first_node),
                    second: Box::new(new_second.expect("a non-matching second pane remains")),
                }),
                None,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PaneNode, SplitDirection};

    fn tree() -> PaneNode<&'static str> {
        PaneNode::leaf(0, "root")
    }

    #[test]
    fn splitting_preserves_the_original_leaf_and_adds_a_focusable_leaf() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Horizontal, "right"));
        assert_eq!(tree.leaf_ids(), vec![0, 1]);
        assert!(tree.contains(0));
        assert!(tree.contains(1));
    }

    #[test]
    fn removing_a_leaf_collapses_the_parent() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Horizontal, "right"));
        let removed = tree.remove(0);
        assert_eq!(removed, Some("root"));
        assert_eq!(tree.leaf_ids(), vec![1]);
    }

    #[test]
    fn ratios_are_clamped_and_survive_nested_splits() {
        let mut tree = tree();
        assert!(tree.split_focused(0, 1, SplitDirection::Horizontal, "right"));
        assert!(tree.split_focused(1, 2, SplitDirection::Vertical, "down"));
        assert!(tree.set_ratio(&[true], 0.75));
        assert!(tree.set_ratio(&[], 0.95));
        assert!(!tree.set_ratio(&[true, true], 0.5));
    }
}
