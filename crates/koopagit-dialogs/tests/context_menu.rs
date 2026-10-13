//! Right-clicking a list row opens its context menu. The rows' TouchArea
//! grabs the press, so each one has to open its DynMenuArea itself.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use koopagit_dialogs::*;
use slint::platform::PointerEventButton;
use slint::{ComponentHandle, ModelRc, VecModel};

/// The first y (at x = 300) whose left click reports `row` through `hits`.
fn row_y(win: &slint::Window, hits: &RefCell<Vec<i32>>, row: i32) -> f32 {
    (0..700)
        .step_by(4)
        .map(|y| y as f32)
        .find(|&y| {
            click(win, 300.0, y, PointerEventButton::Left);
            hits.borrow_mut().drain(..).any(|r| r == row)
        })
        .expect("row not found")
}

#[test]
fn log_row_right_click_opens_the_menu() {
    init();
    let dlg = LogDialog::new().unwrap();
    dlg.set_rows(ModelRc::new(VecModel::from(vec![LogRow::default(); 3])));
    let pressed = Rc::new(RefCell::new(Vec::new()));
    let menus = Rc::new(RefCell::new(Vec::new()));
    let p = pressed.clone();
    dlg.on_row_pressed(move |i, _, _| p.borrow_mut().push(i));
    let m = menus.clone();
    dlg.on_row_menu(move |i| {
        m.borrow_mut().push(i);
        ModelRc::default()
    });
    dlg.show().unwrap();
    let y = row_y(dlg.window(), &pressed, 1);
    assert!(menus.borrow().is_empty());
    click(dlg.window(), 300.0, y, PointerEventButton::Right);
    assert!(!menus.borrow().is_empty());
    assert!(menus.borrow().iter().all(|&r| r == 1));
}

#[test]
fn reflog_row_right_click_opens_the_menu() {
    init();
    let dlg = RefLogDialog::new().unwrap();
    dlg.set_rows(ModelRc::new(VecModel::from(vec![ReflogRow::default(); 3])));
    let clicked = Rc::new(RefCell::new(Vec::new()));
    let right = Rc::new(RefCell::new(Vec::new()));
    let menus = Rc::new(RefCell::new(Vec::new()));
    let c = clicked.clone();
    dlg.on_row_clicked(move |i, _, _| c.borrow_mut().push(i));
    let r = right.clone();
    dlg.on_row_right(move |i| r.borrow_mut().push(i));
    let m = menus.clone();
    dlg.on_menu_for(move |i| {
        m.borrow_mut().push(i);
        ModelRc::default()
    });
    dlg.show().unwrap();
    let y = row_y(dlg.window(), &clicked, 2);
    click(dlg.window(), 300.0, y, PointerEventButton::Right);
    // The row is selected before the menu content is built.
    assert_eq!(*right.borrow(), [2]);
    assert!(!menus.borrow().is_empty());
    assert!(menus.borrow().iter().all(|&r| r == 2));
}
