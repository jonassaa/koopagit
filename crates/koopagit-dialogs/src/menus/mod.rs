//! Data-driven context menus.
//!
//! Menu *content* is decided here, in pure functions that mirror upstream
//! TortoiseGit's conditions (see `docs/spec/spec-menus.md`). Slint only renders
//! it: `ui/dynmenu.slint` repeats a fixed number of [`Block`]s, because a Slint
//! `for` can repeat only one element type. [`pack`] turns a flat entry list into
//! those blocks.

use std::cell::{Cell, RefCell};

use slint::{ModelRc, VecModel};

use crate::{MenuBlock, MenuLeaf, MenuSub};

/// Number of blocks `ui/dynmenu.slint` provides.
pub const MAX_BLOCKS: usize = 24;

/// A clickable entry inside a submenu (or a top-level item).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    pub id: i32,
    pub title: String,
    pub enabled: bool,
}

/// One entry of a menu, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Item(Leaf),
    Separator,
    Sub {
        title: String,
        enabled: bool,
        children: Vec<Leaf>,
    },
}

/// Ids of a menu's actions. Implemented by [`menu_actions!`].
pub trait ActionId: Copy + Sized {
    fn id(self) -> i32;
    fn from_id(id: i32) -> Option<Self>;
}

/// Declares a fieldless action enum with explicit ids and [`ActionId`].
#[macro_export]
macro_rules! menu_actions {
    ($(#[$m:meta])* $vis:vis enum $name:ident { $($(#[$vm:meta])* $variant:ident = $val:expr),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name { $($(#[$vm])* $variant = $val),* }
        impl $crate::menus::ActionId for $name {
            fn id(self) -> i32 { self as i32 }
            fn from_id(id: i32) -> Option<Self> {
                match id { $(x if x == $val => Some($name::$variant),)* _ => None }
            }
        }
    };
}

/// Builds a menu in upstream order. Separators are lazy: [`Menu::finish`]
/// drops leading, trailing and doubled separators and empty submenus, like
/// TortoiseGit's `bAddSeparator` logic.
#[derive(Debug, Default)]
pub struct Menu {
    entries: Vec<Entry>,
}

impl Menu {
    pub fn new() -> Self {
        Menu::default()
    }

    pub fn item<A: ActionId>(&mut self, action: A, title: impl Into<String>) -> &mut Self {
        self.entries.push(Entry::Item(leaf(action, title, true)));
        self
    }

    pub fn item_if<A: ActionId>(
        &mut self,
        show: bool,
        action: A,
        title: impl Into<String>,
    ) -> &mut Self {
        if show {
            self.item(action, title);
        }
        self
    }

    /// An entry that is shown but greyed out.
    pub fn disabled<A: ActionId>(&mut self, action: A, title: impl Into<String>) -> &mut Self {
        self.entries.push(Entry::Item(leaf(action, title, false)));
        self
    }

    pub fn item_enabled<A: ActionId>(
        &mut self,
        enabled: bool,
        action: A,
        title: impl Into<String>,
    ) -> &mut Self {
        self.entries.push(Entry::Item(leaf(action, title, enabled)));
        self
    }

    /// A raw item with a computed id (e.g. "parent N" or "delete ref N").
    pub fn raw(&mut self, id: i32, title: impl Into<String>, enabled: bool) -> &mut Self {
        self.entries.push(Entry::Item(Leaf {
            id,
            title: title.into(),
            enabled,
        }));
        self
    }

    pub fn sep(&mut self) -> &mut Self {
        self.entries.push(Entry::Separator);
        self
    }

    pub fn sub(&mut self, title: impl Into<String>, children: Vec<Leaf>) -> &mut Self {
        self.entries.push(Entry::Sub {
            title: title.into(),
            enabled: true,
            children,
        });
        self
    }

    pub fn finish(self) -> Vec<Entry> {
        normalize(self.entries)
    }
}

pub fn leaf<A: ActionId>(action: A, title: impl Into<String>, enabled: bool) -> Leaf {
    Leaf {
        id: action.id(),
        title: title.into(),
        enabled,
    }
}

/// Drops empty submenus and leading/trailing/doubled separators.
pub fn normalize(entries: Vec<Entry>) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::with_capacity(entries.len());
    for e in entries {
        match e {
            Entry::Separator => {
                if !out.is_empty() && !matches!(out.last(), Some(Entry::Separator)) {
                    out.push(Entry::Separator);
                }
            }
            Entry::Sub { ref children, .. } if children.is_empty() => {}
            other => out.push(other),
        }
    }
    if matches!(out.last(), Some(Entry::Separator)) {
        out.pop();
    }
    out
}

/// A run of items, then submenus, then an optional separator.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Block {
    pub items: Vec<Leaf>,
    pub subs: Vec<(String, bool, Vec<Leaf>)>,
    pub separator: bool,
}

/// Splits a flat menu into blocks that `ui/dynmenu.slint` can render in order.
/// Entries beyond [`MAX_BLOCKS`] blocks are dropped (debug builds panic).
pub fn pack(entries: &[Entry]) -> Vec<Block> {
    let mut blocks: Vec<Block> = vec![Block::default()];
    for e in entries {
        let cur = blocks.last_mut().expect("non-empty");
        match e {
            Entry::Item(l) => {
                if !cur.subs.is_empty() || cur.separator {
                    blocks.push(Block::default());
                }
                blocks.last_mut().expect("non-empty").items.push(l.clone());
            }
            Entry::Sub {
                title,
                enabled,
                children,
            } => {
                if cur.separator {
                    blocks.push(Block::default());
                }
                blocks.last_mut().expect("non-empty").subs.push((
                    title.clone(),
                    *enabled,
                    children.clone(),
                ));
            }
            Entry::Separator => cur.separator = true,
        }
    }
    debug_assert!(
        blocks.len() <= MAX_BLOCKS,
        "menu needs {} blocks, dynmenu.slint has {MAX_BLOCKS}",
        blocks.len()
    );
    blocks.truncate(MAX_BLOCKS);
    blocks
}

fn leaf_model(leaves: &[Leaf]) -> ModelRc<MenuLeaf> {
    ModelRc::new(VecModel::from(
        leaves
            .iter()
            .map(|l| MenuLeaf {
                id: l.id,
                title: l.title.clone().into(),
                enabled: l.enabled,
            })
            .collect::<Vec<_>>(),
    ))
}

/// Converts a menu to the model `DynMenuArea` renders.
pub fn to_model(entries: &[Entry]) -> ModelRc<MenuBlock> {
    let blocks: Vec<MenuBlock> = pack(entries)
        .into_iter()
        .map(|b| MenuBlock {
            items: leaf_model(&b.items),
            subs: ModelRc::new(VecModel::from(
                b.subs
                    .iter()
                    .map(|(title, enabled, children)| MenuSub {
                        title: title.clone().into(),
                        enabled: *enabled,
                        children: leaf_model(children),
                    })
                    .collect::<Vec<_>>(),
            )),
            separator: b.separator,
        })
        .collect();
    ModelRc::new(VecModel::from(blocks))
}

/// Caches the model for one open menu: `DynMenuArea` asks for it several
/// times per open (once per block reference). Invalidate on selection or data change.
#[derive(Default)]
pub struct MenuCache {
    key: Cell<Option<(i32, u64)>>,
    generation: Cell<u64>,
    model: RefCell<ModelRc<MenuBlock>>,
}

impl MenuCache {
    pub fn invalidate(&self) {
        self.generation.set(self.generation.get() + 1);
    }

    pub fn get(&self, row: i32, build: impl FnOnce() -> Vec<Entry>) -> ModelRc<MenuBlock> {
        let key = (row, self.generation.get());
        if self.key.get() != Some(key) {
            *self.model.borrow_mut() = to_model(&build());
            self.key.set(Some(key));
        }
        self.model.borrow().clone()
    }
}

/// Source of `ui/dynmenu.slint` (kept in sync by a test).
pub fn slint_template() -> String {
    let mut s = String::from(
        "// GENERATED by `koopagit_dialogs::menus::slint_template()`; do not edit.\n\
         // Regenerate: KOOPAGIT_REGEN_DYNMENU=1 cargo test -p koopagit-dialogs dynmenu\n\
         //\n\
         // A native context menu whose content comes from Rust (`menus::to_model`).\n\
         // Slint can only repeat one element type per `for`, so the menu is a fixed\n\
         // sequence of blocks: items, then submenus, then an optional separator.\n\
         //\n\
         // A TouchArea child grabs every button press, so the area never sees the\n\
         // right-click: on a right press the TouchArea must call\n\
         // `parent.show({ x: self.mouse-x, y: self.mouse-y })` itself.\n\n\
         export struct MenuLeaf { id: int, title: string, enabled: bool }\n\
         export struct MenuSub { title: string, enabled: bool, children: [MenuLeaf] }\n\
         export struct MenuBlock { items: [MenuLeaf], subs: [MenuSub], separator: bool }\n\n\
         export component DynMenuArea inherits ContextMenuArea {\n\
         \x20   // Evaluated each time the menu opens.\n\
         \x20   pure callback blocks() -> [MenuBlock];\n\
         \x20   callback chosen(int);\n\
         \x20   Menu {\n",
    );
    for k in 0..MAX_BLOCKS {
        s.push_str(&format!(
            "        for e in root.blocks()[{k}].items: MenuItem {{ title: e.title; enabled: e.enabled; activated => {{ root.chosen(e.id); }} }}\n\
             \x20       for m in root.blocks()[{k}].subs: Menu {{\n\
             \x20           title: m.title;\n\
             \x20           enabled: m.enabled;\n\
             \x20           for c in m.children: MenuItem {{ title: c.title; enabled: c.enabled; activated => {{ root.chosen(c.id); }} }}\n\
             \x20       }}\n\
             \x20       if root.blocks()[{k}].separator: MenuSeparator {{ }}\n"
        ));
    }
    s.push_str("    }\n}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    menu_actions! {
        enum A { One = 1, Two = 2, Three = 3, Four = 4 }
    }

    #[test]
    fn lazy_separators_and_empty_submenus() {
        let mut m = Menu::new();
        m.sep()
            .item(A::One, "one")
            .sep()
            .sep()
            .sub("empty", vec![])
            .item_if(false, A::Two, "hidden")
            .sep()
            .disabled(A::Three, "three")
            .sep();
        let e = m.finish();
        assert_eq!(e.len(), 3);
        assert!(matches!(&e[0], Entry::Item(l) if l.title == "one"));
        assert_eq!(e[1], Entry::Separator);
        assert!(matches!(&e[2], Entry::Item(l) if !l.enabled));
        assert_eq!(A::from_id(4), Some(A::Four));
        assert_eq!(A::from_id(9), None);
    }

    #[test]
    fn packing_preserves_order() {
        let mut m = Menu::new();
        m.item(A::One, "a")
            .sub("S1", vec![leaf(A::Two, "s", true)])
            .item(A::Three, "b")
            .sep()
            .sub("S2", vec![leaf(A::Four, "t", true)])
            .item(A::One, "c");
        let blocks = pack(&m.finish());
        let shape: Vec<(usize, usize, bool)> = blocks
            .iter()
            .map(|b| (b.items.len(), b.subs.len(), b.separator))
            .collect();
        assert_eq!(
            shape,
            [(1, 1, false), (1, 0, true), (0, 1, false), (1, 0, false)]
        );
    }

    #[test]
    fn dynmenu_slint_is_generated() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/ui/dynmenu.slint");
        let expected = slint_template();
        if std::env::var_os("KOOPAGIT_REGEN_DYNMENU").is_some() {
            std::fs::write(path, &expected).unwrap();
        }
        let actual = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            actual == expected,
            "ui/dynmenu.slint is stale; rerun with KOOPAGIT_REGEN_DYNMENU=1"
        );
    }
}

pub mod filediff;
pub mod repobrowser;
pub mod worktree;
// Menu builders (worker C).
pub mod commitmsg;
pub mod rebase;
pub mod refbrowse;
pub mod reflog;
pub mod viewers;
