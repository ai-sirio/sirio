/// Symbolic terminal keys and their xterm byte sequences.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalKey {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

impl TerminalKey {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Enter => b"\r",
            Self::Tab => b"\t",
            Self::Escape => b"\x1b",
            Self::Backspace => b"\x7f",
            Self::Delete => b"\x1b[3~",
            Self::ArrowUp => b"\x1b[A",
            Self::ArrowDown => b"\x1b[B",
            Self::ArrowLeft => b"\x1b[D",
            Self::ArrowRight => b"\x1b[C",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Left,
    Right,
    Up,
    Down,
}

impl SplitDirection {
    fn axis(self) -> SplitAxis {
        match self {
            Self::Left | Self::Right => SplitAxis::Horizontal,
            Self::Up | Self::Down => SplitAxis::Vertical,
        }
    }

    fn new_first(self) -> bool {
        matches!(self, Self::Left | Self::Up)
    }
}

/// An immutable terminal split tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SplitTree {
    Leaf {
        id: String,
    },
    Split {
        axis: SplitAxis,
        fraction_millis: u16,
        first: Box<Self>,
        second: Box<Self>,
    },
}

impl SplitTree {
    pub fn leaf(id: impl Into<String>) -> Self {
        Self::Leaf { id: id.into() }
    }

    pub fn split_leaf(
        &self,
        target: &str,
        new_id: impl Into<String>,
        direction: SplitDirection,
    ) -> Option<Self> {
        let new_id = new_id.into();
        let new_leaf = Self::Leaf { id: new_id.clone() };
        match self {
            Self::Leaf { id } if id == target => {
                let old = Box::new(self.clone());
                let new = Box::new(new_leaf);
                Some(if direction.new_first() {
                    Self::Split {
                        axis: direction.axis(),
                        fraction_millis: 500,
                        first: new,
                        second: old,
                    }
                } else {
                    Self::Split {
                        axis: direction.axis(),
                        fraction_millis: 500,
                        first: old,
                        second: new,
                    }
                })
            }
            Self::Leaf { .. } => None,
            Self::Split {
                axis,
                fraction_millis,
                first,
                second,
            } => {
                if let Some(updated) = first.split_leaf(target, new_id.clone(), direction) {
                    return Some(Self::Split {
                        axis: *axis,
                        fraction_millis: *fraction_millis,
                        first: Box::new(updated),
                        second: second.clone(),
                    });
                }
                second
                    .split_leaf(target, new_id, direction)
                    .map(|updated| Self::Split {
                        axis: *axis,
                        fraction_millis: *fraction_millis,
                        first: first.clone(),
                        second: Box::new(updated),
                    })
            }
        }
    }

    pub fn remove_leaf(&self, target: &str) -> Option<Self> {
        match self {
            Self::Leaf { id } if id == target => None,
            Self::Leaf { .. } => Some(self.clone()),
            Self::Split {
                axis,
                fraction_millis,
                first,
                second,
            } => {
                let first = first.remove_leaf(target);
                let second = second.remove_leaf(target);
                match (first, second) {
                    (None, None) => None,
                    (Some(tree), None) | (None, Some(tree)) => Some(tree),
                    (Some(first), Some(second)) => Some(Self::Split {
                        axis: *axis,
                        fraction_millis: *fraction_millis,
                        first: Box::new(first),
                        second: Box::new(second),
                    }),
                }
            }
        }
    }

    pub fn leaf_ids(&self) -> Vec<&str> {
        match self {
            Self::Leaf { id } => vec![id],
            Self::Split { first, second, .. } => {
                let mut ids = first.leaf_ids();
                ids.extend(second.leaf_ids());
                ids
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic_keys_use_documented_sequences() {
        assert_eq!(TerminalKey::Enter.bytes(), b"\r");
        assert_eq!(TerminalKey::Tab.bytes(), b"\t");
        assert_eq!(TerminalKey::Escape.bytes(), b"\x1b");
        assert_eq!(TerminalKey::Backspace.bytes(), b"\x7f");
        assert_eq!(TerminalKey::Delete.bytes(), b"\x1b[3~");
        assert_eq!(TerminalKey::ArrowRight.bytes(), b"\x1b[C");
    }

    #[test]
    fn split_and_remove_collapse_parent_nodes_and_enumerate_leaves() {
        let tree = SplitTree::leaf("a");
        let tree = tree.split_leaf("a", "b", SplitDirection::Right).unwrap();
        let tree = tree.split_leaf("a", "c", SplitDirection::Down).unwrap();
        assert_eq!(tree.leaf_ids(), vec!["a", "c", "b"]);
        let tree = tree.remove_leaf("c").unwrap();
        assert_eq!(tree.leaf_ids(), vec!["a", "b"]);
        let tree = tree.remove_leaf("a").unwrap();
        assert_eq!(tree, SplitTree::leaf("b"));
    }
}
