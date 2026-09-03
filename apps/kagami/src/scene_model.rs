//! Pure data model for the scene tree shown in the left panel and
//! referenced by the right-panel inspector. No UI dependency.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKind {
    Category,
    Planet,
    Probe,
    SlicePlane,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            rotation: [0.0; 3],
        }
    }
}

#[derive(Debug, Clone)]
pub struct SceneNode {
    pub id: NodeId,
    pub name: String,
    pub kind: ObjectKind,
    pub visible: bool,
    pub transform: Transform,
    pub children: Vec<SceneNode>,
}

impl SceneNode {
    pub fn new(id: NodeId, name: impl Into<String>, kind: ObjectKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            visible: true,
            transform: Transform::default(),
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<SceneNode>) -> Self {
        self.children = children;
        self
    }

    fn find(&self, id: NodeId) -> Option<&SceneNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(id))
    }

    fn find_mut(&mut self, id: NodeId) -> Option<&mut SceneNode> {
        if self.id == id {
            return Some(self);
        }
        self.children
            .iter_mut()
            .find_map(|child| child.find_mut(id))
    }

    /// Whether this node or any of its descendants matches `query`
    /// (case-insensitive substring match).
    fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        if self.name.to_lowercase().contains(&query.to_lowercase()) {
            return true;
        }
        self.children.iter().any(|child| child.matches(query))
    }
}

#[derive(Debug, Clone, Default)]
pub struct SceneTree {
    pub roots: Vec<SceneNode>,
}

impl SceneTree {
    pub fn find(&self, id: NodeId) -> Option<&SceneNode> {
        self.roots.iter().find_map(|root| root.find(id))
    }

    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut SceneNode> {
        self.roots.iter_mut().find_map(|root| root.find_mut(id))
    }

    pub fn toggle_visibility(&mut self, id: NodeId) {
        if let Some(node) = self.find_mut(id) {
            node.visible = !node.visible;
        }
    }

    /// Roots that match `query` themselves or have a matching descendant,
    /// so ancestor categories stay visible while filtering.
    pub fn filtered_roots(&self, query: &str) -> Vec<&SceneNode> {
        self.roots
            .iter()
            .filter(|root| root.matches(query))
            .collect()
    }

    pub fn node_matches(node: &SceneNode, query: &str) -> bool {
        node.matches(query)
    }
}
