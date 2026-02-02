//! Custom tree component replacing yewprint's Tree.
//!
//! This module provides a simple tree UI component with icons.

pub mod icon;
pub mod tree_data;
pub mod tree_view;

pub use icon::Icon;
pub use id_tree::{self, InsertBehavior, Node, NodeId, TreeBuilder};
pub use tree_data::{NodeData, TreeData};
pub use tree_view::Tree;
