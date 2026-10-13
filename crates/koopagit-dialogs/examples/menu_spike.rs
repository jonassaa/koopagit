//! Phase 0.1 spike: can Slint menus be built from a model computed in Rust?
use std::cell::Cell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

slint::slint! {
    export struct MenuLeaf { id: int, title: string, enabled: bool }
    export struct MenuSub { title: string, enabled: bool, children: [MenuLeaf] }
    export struct MenuBlock { items: [MenuLeaf], subs: [MenuSub], separator: bool }

    component DynMenuArea inherits ContextMenuArea {
        in property <[MenuBlock]> blocks;
        pure callback blocks-for() -> [MenuBlock];
        callback chosen(int);
        Menu {
            for e in root.blocks-for()[0].items: MenuItem { title: e.title; enabled: e.enabled; activated => { root.chosen(e.id); } }
            for s in root.blocks-for()[0].subs: Menu {
                title: s.title;
                for c in s.children: MenuItem { title: c.title; enabled: c.enabled; activated => { root.chosen(c.id); } }
            }
            if root.blocks-for()[0].separator: MenuSeparator {}
            for e in root.blocks-for()[1].items: MenuItem { title: e.title; enabled: e.enabled; activated => { root.chosen(e.id); } }
        }
    }

    export component Spike inherits Window {
        width: 360px; height: 260px;
        pure callback menu-for(int) -> [MenuBlock];
        callback activated(int);
        in-out property <int> current-row;
        area := DynMenuArea {
            blocks-for() => { return root.menu-for(root.current-row); }
            chosen(id) => { root.activated(id); }
            Rectangle { background: #224; }
        }
        public function open() { area.show({x: 20px, y: 20px}); }
    }
}

fn leaf(id: i32, t: &str) -> MenuLeaf {
    MenuLeaf {
        id,
        title: t.into(),
        enabled: true,
    }
}

fn main() {
    let s = Spike::new().unwrap();
    let calls = Rc::new(Cell::new(0));
    {
        let calls = calls.clone();
        s.on_menu_for(move |row| {
            calls.set(calls.get() + 1);
            eprintln!("menu_for(row={row}) call #{}", calls.get());
            let b0 = MenuBlock {
                items: ModelRc::new(VecModel::from(vec![
                    leaf(1, &format!("Row {row}: Compare")),
                    leaf(2, "Show log"),
                ])),
                subs: ModelRc::new(VecModel::from(vec![MenuSub {
                    title: "Copy to clipboard".into(),
                    enabled: true,
                    children: ModelRc::new(VecModel::from(vec![
                        leaf(20, "SHA-1"),
                        leaf(21, "Message"),
                    ])),
                }])),
                separator: true,
            };
            let b1 = MenuBlock {
                items: ModelRc::new(VecModel::from(vec![leaf(3, "Delete")])),
                subs: ModelRc::default(),
                separator: false,
            };
            ModelRc::new(VecModel::from(vec![b0, b1]))
        });
    }
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/spike.png".into());
    let weak = s.as_weak();
    let calls2 = calls.clone();
    slint::Timer::single_shot(std::time::Duration::from_millis(300), move || {
        let s = weak.upgrade().unwrap();
        eprintln!("calls before first open: {}", calls2.get());
        s.set_current_row(7);
        s.invoke_open();
        eprintln!("calls after first open: {}", calls2.get());
        let weak = s.as_weak();
        let calls3 = calls2.clone();
        slint::Timer::single_shot(std::time::Duration::from_millis(700), move || {
            let s = weak.upgrade().unwrap();
            if let Ok(buf) = s.window().take_snapshot() {
                eprintln!(
                    "snapshot {}x{} (menu visible only if Slint-rendered)",
                    buf.width(),
                    buf.height()
                );
                let px = buf.as_slice();
                let bright = px
                    .iter()
                    .filter(|p| p.r > 200 && p.g > 200 && p.b > 200)
                    .count();
                eprintln!("bright pixels: {bright}");
            }
            s.set_current_row(9);
            s.invoke_open();
            eprintln!("calls after second open: {}", calls3.get());
            let _ = out;
            let _ = slint::quit_event_loop();
        });
    });
    s.run().unwrap();
    let _ = s.get_current_row();
    let _ = ModelRc::<MenuLeaf>::default().row_count();
}
