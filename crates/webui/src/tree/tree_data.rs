//! Tree data structures compatible with the yewprint API.

use super::Icon;
use std::cell::RefCell;
use std::rc::Rc;
use yew::Html;

/// Data associated with each tree node.
#[derive(Clone)]
pub struct NodeData<T: Clone> {
    /// Icon to display for this node.
    pub icon: Icon,
    /// The label/content to display.
    pub label: Html,
    /// Whether this node can be expanded (has a caret).
    pub has_caret: bool,
    /// Whether this node is currently expanded.
    pub is_expanded: bool,
    /// Whether this node is currently selected.
    pub is_selected: bool,
    /// Custom data associated with this node.
    pub data: T,
}

impl<T: Clone + Default> Default for NodeData<T> {
    fn default() -> Self {
        Self {
            icon: Icon::default(),
            label: Html::default(),
            has_caret: false,
            is_expanded: false,
            is_selected: false,
            data: T::default(),
        }
    }
}

impl<T: Clone + PartialEq> PartialEq for NodeData<T> {
    fn eq(&self, other: &Self) -> bool {
        self.icon == other.icon
            && self.has_caret == other.has_caret
            && self.is_expanded == other.is_expanded
            && self.is_selected == other.is_selected
            && self.data == other.data
        // Note: Html doesn't implement PartialEq, so we skip label comparison
    }
}

/// Wrapper around id_tree::Tree with RefCell for interior mutability.
#[derive(Clone)]
pub struct TreeData<T: Clone>(Rc<RefCell<id_tree::Tree<NodeData<T>>>>);

impl<T: Clone> TreeData<T> {
    /// Borrow the inner tree mutably.
    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, id_tree::Tree<NodeData<T>>> {
        self.0.borrow_mut()
    }

    /// Borrow the inner tree immutably.
    pub fn borrow(&self) -> std::cell::Ref<'_, id_tree::Tree<NodeData<T>>> {
        self.0.borrow()
    }
}

impl<T: Clone> From<id_tree::Tree<NodeData<T>>> for TreeData<T> {
    fn from(tree: id_tree::Tree<NodeData<T>>) -> Self {
        Self(Rc::new(RefCell::new(tree)))
    }
}

impl<T: Clone + PartialEq> PartialEq for TreeData<T> {
    fn eq(&self, _other: &Self) -> bool {
        // Always return false to force re-render when parent re-renders.
        // This is necessary because TreeData uses interior mutability (RefCell),
        // which Yew's PartialEq-based change detection cannot track.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree_data_partial_eq_always_false() {
        // TreeData::eq always returns false to force re-renders
        let tree1: TreeData<()> = id_tree::TreeBuilder::new().build().into();
        let tree2 = tree1.clone();

        // Even cloned trees (same Rc) should not be equal
        // This is intentional to force Yew to re-render
        assert!(!tree1.eq(&tree2), "TreeData::eq should always return false");
    }

    #[test]
    fn test_tree_data_borrow() {
        let tree: TreeData<i32> = id_tree::TreeBuilder::new().build().into();

        // Should be able to borrow immutably
        let _borrowed = tree.borrow();
    }

    #[test]
    fn test_tree_data_borrow_mut() {
        let tree: TreeData<i32> = id_tree::TreeBuilder::new().build().into();

        // Should be able to borrow mutably
        let _borrowed = tree.borrow_mut();
    }
}
