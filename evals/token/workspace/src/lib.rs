//! Tiny widget library used by the token bench (T8.5). Small enough to
//! read in full, big enough that truncation and outlines matter elsewhere.

use std::collections::HashMap;

pub mod auth;

/// A widget on the canvas.
pub struct Widget {
    /// Stable identifier.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// Whether the widget is visible.
    pub visible: bool,
}

impl Widget {
    /// Creates a visible widget.
    pub fn new(id: u64, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            visible: true,
        }
    }

    /// Hides the widget.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Shows the widget.
    pub fn show(&mut self) {
        self.visible = true;
    }
}

/// A canvas holding every widget by id.
pub struct Canvas {
    widgets: HashMap<u64, Widget>,
}

impl Canvas {
    /// An empty canvas.
    pub fn new() -> Self {
        Self {
            widgets: HashMap::new(),
        }
    }

    /// Adds a widget, replacing any with the same id.
    pub fn add(&mut self, widget: Widget) {
        self.widgets.insert(widget.id, widget);
    }

    /// Removes a widget, returning whether one was there.
    pub fn remove(&mut self, id: u64) -> bool {
        self.widgets.remove(&id).is_some()
    }

    /// Looks a widget up by id.
    pub fn get(&self, id: u64) -> Option<&Widget> {
        self.widgets.get(&id)
    }

    /// Counts visible widgets.
    pub fn visible_count(&self) -> usize {
        self.widgets.values().filter(|w| w.visible).count()
    }
}
