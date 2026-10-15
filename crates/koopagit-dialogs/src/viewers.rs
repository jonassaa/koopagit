//! Viewer windows: two-way diff and conflict editor (TortoiseGitMerge),
//! unified diff (TortoiseGitUDiff) and blame (TortoiseGitBlame).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::blame::{Blame, BlameOptions};
use koopagit_core::conflict::{self, Choice, Segment};
use koopagit_core::diff::{self, AlignedRow, DiffOptions, RowKind, Whitespace};
use koopagit_core::launcher::Invocation;
use koopagit_core::mergeview::{BlockOp, DiffDoc, Hunk};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::menus::viewers::{self as vmenu, DiffMenuInput, Pane, ViewAction};

use crate::{
    BlameRow, BlameViewer, ConflictRow, ConflictViewer, DialogResult, DiffRow, DiffViewer,
    UDiffLine, UDiffViewer, ViewerStyle, spawn_launcher,
};

/// Monospace font family per platform.
fn mono_font() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(windows) {
        "Consolas"
    } else {
        "DejaVu Sans Mono"
    }
}

/// Applies the platform monospace font to a viewer window.
macro_rules! set_style {
    ($dlg:expr) => {
        $dlg.global::<ViewerStyle>().set_mono(mono_font().into())
    };
}

/// Reads a text file, tolerating invalid UTF-8 and a missing file.
fn read_text(p: &Path) -> String {
    std::fs::read(p)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// Expands tabs to 4 columns for display.
fn display(line: &str) -> String {
    line.trim_end_matches('\r').replace('\t', "    ")
}

fn row_height() -> f32 {
    18.0
}

/// The diff options chosen in the viewer's toolbar.
fn diff_options(d: &DiffViewer) -> DiffOptions {
    DiffOptions {
        whitespace: match d.get_whitespace() {
            1 => Whitespace::IgnoreChanges,
            2 => Whitespace::IgnoreAll,
            _ => Whitespace::Compare,
        },
        ignore_case: d.get_ignore_case(),
        ignore_eol: true,
    }
}

// ---------------------------------------------------------------------------
// Save prompt on exit

/// IDS_WARNMODIFIEDLOOSECHANGES.
pub const SAVE_PROMPT: &str = "There are unsaved modifications!\nDo you want to save your changes?";

/// An answer to the exit question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveChoice {
    SaveLeft,
    SaveRight,
    SaveAll,
    /// The right (two-way) or merged (three-way) file.
    Save,
    Quit,
    Cancel,
}

/// The exit question's answers (`CMainFrame::CheckForSave`, MainFrm.cpp:2613,
/// with IDS_ASKFORSAVE_* captions): both sides of a diff modified offer each
/// side and "Save all"; otherwise one save entry. Empty when nothing changed.
pub fn save_prompt(left_modified: bool, right_modified: bool) -> Vec<(&'static str, SaveChoice)> {
    use SaveChoice as C;
    let mut out = match (left_modified, right_modified) {
        (false, false) => return Vec::new(),
        (true, true) => vec![
            ("Save Left File", C::SaveLeft),
            ("Save Right File", C::SaveRight),
            ("Save all", C::SaveAll),
        ],
        (true, false) => vec![("Save Left File", C::SaveLeft)],
        (false, true) => vec![("Save", C::Save)],
    };
    out.extend([("Quit", C::Quit), ("Cancel", C::Cancel)]);
    out
}

/// The question in the conflict viewer's notice bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConflictAsk {
    None,
    /// "Mark the file as resolved?" after a save.
    Resolve,
    /// The exit question.
    Exit,
}

fn notice_buttons(choices: &[(&'static str, SaveChoice)]) -> ModelRc<SharedString> {
    let labels: Vec<SharedString> = choices.iter().map(|(l, _)| (*l).into()).collect();
    ModelRc::new(VecModel::from(labels))
}

// ---------------------------------------------------------------------------
// Two-way diff

/// Builds the rows of the side-by-side view.
pub fn diff_rows(left: &[&str], right: &[&str], rows: &[AlignedRow]) -> Vec<DiffRow> {
    rows.iter()
        .map(|r| DiffRow {
            lno: r
                .left
                .map(|i| (i + 1).to_string())
                .unwrap_or_default()
                .into(),
            ltext: r.left.map(|i| display(left[i])).unwrap_or_default().into(),
            rno: r
                .right
                .map(|i| (i + 1).to_string())
                .unwrap_or_default()
                .into(),
            rtext: r
                .right
                .map(|i| display(right[i]))
                .unwrap_or_default()
                .into(),
            kind: match r.kind {
                RowKind::Equal => 0,
                RowKind::Removed => 1,
                RowKind::Added => 2,
                RowKind::Changed => 3,
            },
        })
        .collect()
}

/// Shows two files side by side. `line` scrolls to that (1-based) right-hand line.
pub fn run_diff(
    left: &Path,
    right: &Path,
    left_title: &str,
    right_title: &str,
    line: Option<usize>,
) -> DialogResult {
    let dlg = DiffViewer::new()?;
    set_style!(dlg);
    let left_raw = read_text(left);
    let right_raw = read_text(right);
    let doc = Rc::new(RefCell::new(DiffDoc::new(&left_raw, &right_raw)));
    let eol = |t: &str| if t.contains("\r\n") { "\r\n" } else { "\n" };
    let (left_eol, right_eol) = (eol(&left_raw), eol(&right_raw));
    dlg.set_dialog_title(format!("{left_title} - {right_title} - KoopaGitMerge").into());
    dlg.set_left_title(left_title.into());
    dlg.set_right_title(right_title.into());

    let blocks: Rc<RefCell<(Vec<usize>, Vec<AlignedRow>)>> = Rc::default();
    let model = Rc::new(VecModel::<DiffRow>::default());
    dlg.set_rows(ModelRc::from(model.clone()));
    let menu_cache = Rc::new(crate::menus::MenuCache::default());

    let recompute = {
        let weak = dlg.as_weak();
        let blocks = blocks.clone();
        let doc = doc.clone();
        let menu_cache = menu_cache.clone();
        move || {
            let Some(d) = weak.upgrade() else { return };
            let opts = diff_options(&d);
            let doc = doc.borrow();
            menu_cache.invalidate();
            d.set_can_save(doc.left_modified || doc.right_modified);
            let a: Vec<&str> = doc.left.iter().map(String::as_str).collect();
            let b: Vec<&str> = doc.right.iter().map(String::as_str).collect();
            let rows = diff::align(&diff::diff_lines(&a, &b, &opts));
            let starts = diff::difference_blocks(&rows);
            model.set_vec(diff_rows(&a, &b, &rows));
            d.set_status_text(
                if starts.is_empty() {
                    "The files are identical".to_owned()
                } else {
                    format!(
                        "{} difference(s)  -  Left: {} lines  Right: {} lines",
                        starts.len(),
                        a.len(),
                        b.len()
                    )
                }
                .into(),
            );
            *blocks.borrow_mut() = (starts, rows);
        }
    };
    recompute();
    {
        let recompute = recompute.clone();
        dlg.on_options_changed(recompute);
    }
    // Block menus and editing (spec-menus.md §8.3/§8.4, two-pane).
    let writable = |p: &Path| p.is_file() && !p.starts_with(std::env::temp_dir());
    let (left_writable, right_writable) = (writable(left), writable(right));
    let hunk_at_row = {
        let blocks = blocks.clone();
        let doc = doc.clone();
        let weak = dlg.as_weak();
        move |row: i32| -> Option<Hunk> {
            let d = weak.upgrade()?;
            let b = blocks.borrow();
            let r = b.1.get(usize::try_from(row).ok()?)?;
            if r.kind == RowKind::Equal {
                return None;
            }
            doc.borrow().hunk_at(&diff_options(&d), r.left, r.right)
        }
    };
    {
        let hunk_at_row = hunk_at_row.clone();
        let doc = doc.clone();
        let menu_cache = menu_cache.clone();
        dlg.on_pane_menu(move |row, pane| {
            let hunk = hunk_at_row(row);
            let marked = hunk.is_some_and(|h| doc.borrow().is_marked(h));
            menu_cache.get(row * 2 + pane, || {
                vmenu::two_pane(
                    if pane == 0 { Pane::Left } else { Pane::Right },
                    &DiffMenuInput {
                        in_block: hunk.is_some(),
                        left_writable,
                        right_writable,
                        marked,
                    },
                )
            })
        });
    }
    {
        let doc = doc.clone();
        let recompute = recompute.clone();
        let weak = dlg.as_weak();
        dlg.on_pane_menu_action(move |row, pane, id| {
            let Some(d) = weak.upgrade() else { return };
            let Some(action) = <ViewAction as crate::menus::ActionId>::from_id(id) else {
                return;
            };
            let hunk = hunk_at_row(row);
            let opts = diff_options(&d);
            let mut doc = doc.borrow_mut();
            let op = |h: Option<Hunk>, op: BlockOp, doc: &mut DiffDoc| {
                if let Some(h) = h {
                    doc.apply(h, op);
                }
            };
            match action {
                ViewAction::UseLeftBlock => op(hunk, BlockOp::UseLeftBlock, &mut doc),
                ViewAction::UseBothLeftFirst => op(hunk, BlockOp::UseBothLeftFirst, &mut doc),
                ViewAction::UseBothRightFirst => op(hunk, BlockOp::UseBothRightFirst, &mut doc),
                ViewAction::PrependRightToLeft => op(hunk, BlockOp::PrependRightToLeft, &mut doc),
                ViewAction::ReplaceLeftByRight => op(hunk, BlockOp::ReplaceLeftByRight, &mut doc),
                ViewAction::AppendRightToLeft => op(hunk, BlockOp::AppendRightToLeft, &mut doc),
                ViewAction::UseLeftFile => doc.use_left_file(),
                ViewAction::UseRightFile => doc.use_right_file(),
                ViewAction::MarkBlock => {
                    if let Some(h) = hunk {
                        doc.mark(h, true);
                    }
                }
                ViewAction::UnmarkBlock => {
                    if let Some(h) = hunk {
                        doc.mark(h, false);
                    }
                }
                ViewAction::LeaveOnlyMarked => doc.leave_only_marked(&opts),
                ViewAction::Copy => {
                    if let Some(h) = hunk {
                        let lines = if pane == 0 {
                            &doc.left[h.left..h.left + h.left_len]
                        } else {
                            &doc.right[h.right..h.right + h.right_len]
                        };
                        let _ = crate::clipboard::copy(&lines.join("\n"));
                    }
                }
                _ => {}
            }
            drop(doc);
            recompute();
        });
    }
    // F12 / Alt+Right (ID_EDIT_USELEFTBLOCK): enabled only when the right view
    // is the writable target and a difference is selected.
    {
        let weak = dlg.as_weak();
        dlg.on_use_left_block(move || {
            let Some(d) = weak.upgrade() else { return };
            if right_writable && d.get_current() >= 0 {
                let id = crate::menus::ActionId::id(ViewAction::UseLeftBlock);
                d.invoke_pane_menu_action(d.get_current(), 1, id);
            }
        });
    }
    // Saves the modified sides among `left` / `right`; `false` if a write failed.
    let save: Rc<dyn Fn(bool, bool) -> bool> = {
        let doc = doc.clone();
        let weak = dlg.as_weak();
        let (left, right) = (left.to_owned(), right.to_owned());
        Rc::new(move |save_left, save_right| {
            let Some(d) = weak.upgrade() else {
                return false;
            };
            let mut doc = doc.borrow_mut();
            let mut errors = Vec::new();
            if save_right && doc.right_modified {
                match std::fs::write(&right, doc.right_text(right_eol)) {
                    Ok(()) => doc.right_modified = false,
                    Err(e) => errors.push(format!("{}: {e}", right.display())),
                }
            }
            if save_left && doc.left_modified {
                match std::fs::write(&left, doc.left_text(left_eol)) {
                    Ok(()) => doc.left_modified = false,
                    Err(e) => errors.push(format!("{}: {e}", left.display())),
                }
            }
            d.set_can_save(doc.left_modified || doc.right_modified);
            d.set_status_text(
                if errors.is_empty() {
                    "Saved.".to_owned()
                } else {
                    errors.join("; ")
                }
                .into(),
            );
            errors.is_empty()
        })
    };
    {
        let save = save.clone();
        dlg.on_save(move || {
            save(true, true);
        });
    }
    // Exit (keys, Close, the window's close button): ask first when there are
    // unsaved modifications (`CMainFrame::OnClose` → `CheckForSave`).
    let prompt: Rc<RefCell<Vec<(&'static str, SaveChoice)>>> = Rc::default();
    let request_close: Rc<dyn Fn() -> bool> = {
        let doc = doc.clone();
        let weak = dlg.as_weak();
        let prompt = prompt.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else {
                return true;
            };
            let choices = {
                let doc = doc.borrow();
                save_prompt(doc.left_modified, doc.right_modified)
            };
            if choices.is_empty() {
                let _ = d.hide();
                return true;
            }
            d.set_notice_text(SAVE_PROMPT.into());
            d.set_notice_buttons(notice_buttons(&choices));
            *prompt.borrow_mut() = choices;
            false
        })
    };
    {
        let request_close = request_close.clone();
        dlg.on_close_viewer(move || {
            request_close();
        });
    }
    dlg.window().on_close_requested(move || {
        if request_close() {
            slint::CloseRequestResponse::HideWindow
        } else {
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    {
        let weak = dlg.as_weak();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            d.set_notice_text(SharedString::new());
            let choice = prompt.borrow().get(i as usize).map(|(_, c)| *c);
            let close = match choice {
                Some(SaveChoice::SaveLeft) => save(true, false),
                Some(SaveChoice::SaveRight | SaveChoice::Save) => save(false, true),
                Some(SaveChoice::SaveAll) => save(true, true),
                Some(SaveChoice::Quit) => true,
                Some(SaveChoice::Cancel) | None => false,
            };
            if close {
                let _ = d.hide();
            }
        });
    }
    let jump = {
        let weak = dlg.as_weak();
        move |row: usize| {
            if let Some(d) = weak.upgrade() {
                d.set_current(row as i32);
                d.set_scroll_y(-(row.saturating_sub(3) as f32) * row_height());
            }
        }
    };
    {
        let blocks = blocks.clone();
        let weak = dlg.as_weak();
        let jump = jump.clone();
        dlg.on_next_difference(move || {
            let Some(d) = weak.upgrade() else { return };
            let cur = usize::try_from(d.get_current()).ok();
            if let Some(n) = diff::next_block(&blocks.borrow().0, cur) {
                jump(n);
            }
        });
    }
    {
        let blocks = blocks.clone();
        let weak = dlg.as_weak();
        let jump = jump.clone();
        dlg.on_previous_difference(move || {
            let Some(d) = weak.upgrade() else { return };
            let cur = usize::try_from(d.get_current()).ok();
            if let Some(n) = diff::prev_block(&blocks.borrow().0, cur) {
                jump(n);
            }
        });
    }
    // Initial position: the requested line, else the first difference.
    let start = match line {
        Some(l) => blocks
            .borrow()
            .1
            .iter()
            .position(|r| r.right == Some(l.saturating_sub(1))),
        None => blocks.borrow().0.first().copied(),
    };
    if let Some(r) = start {
        jump(r);
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Conflict editor

/// Rows of the conflict view: common lines on both sides, then for each block a
/// header row and the theirs (left) / mine (right) lines padded to equal length.
pub fn conflict_rows(segments: &[Segment], choices: &[Choice]) -> Vec<ConflictRow> {
    let mut rows = Vec::new();
    let mut k = 0;
    for s in segments {
        match s {
            Segment::Common(lines) => rows.extend(lines.iter().map(|l| ConflictRow {
                ltext: display(l.trim_end_matches('\n')).into(),
                rtext: display(l.trim_end_matches('\n')).into(),
                kind: 0,
                block: -1,
                resolved: false,
                header: SharedString::new(),
            })),
            Segment::Conflict(c) => {
                let choice = choices.get(k).copied().unwrap_or_default();
                let state = match choice {
                    Choice::Unresolved => "unresolved",
                    Choice::Mine => "using 'mine'",
                    Choice::Theirs => "using 'theirs'",
                    Choice::MineThenTheirs => "using 'mine' before 'theirs'",
                    Choice::TheirsThenMine => "using 'theirs' before 'mine'",
                };
                rows.push(ConflictRow {
                    ltext: SharedString::new(),
                    rtext: SharedString::new(),
                    kind: 2,
                    block: k as i32,
                    resolved: choice != Choice::Unresolved,
                    header: format!(
                        "Conflict {} ({state}) - theirs: {} / mine: {}",
                        k + 1,
                        c.theirs_label,
                        c.ours_label
                    )
                    .into(),
                });
                for i in 0..c.ours.len().max(c.theirs.len()) {
                    let t = |v: &Vec<String>| {
                        v.get(i)
                            .map(|l| display(l.trim_end_matches('\n')))
                            .unwrap_or_default()
                    };
                    rows.push(ConflictRow {
                        ltext: t(&c.theirs).into(),
                        rtext: t(&c.ours).into(),
                        kind: 1,
                        block: k as i32,
                        resolved: choice != Choice::Unresolved,
                        header: SharedString::new(),
                    });
                }
                k += 1;
            }
        }
    }
    rows
}

/// Merged preview: resolved blocks rendered, unresolved ones kept with markers.
fn merged_preview(segments: &[Segment], choices: &[Choice]) -> String {
    let mut out = String::new();
    let mut k = 0;
    for s in segments {
        match s {
            Segment::Common(lines) => lines.iter().for_each(|l| out.push_str(l)),
            Segment::Conflict(c) => {
                let choice = choices.get(k).copied().unwrap_or_default();
                if choice == Choice::Unresolved {
                    out.push_str(&format!("<<<<<<< {}\n", c.ours_label));
                    c.ours.iter().for_each(|l| out.push_str(l));
                    out.push_str("=======\n");
                    c.theirs.iter().for_each(|l| out.push_str(l));
                    out.push_str(&format!(">>>>>>> {}\n", c.theirs_label));
                } else {
                    let one = vec![Segment::Conflict(c.clone())];
                    out.push_str(&conflict::render(&one, &[choice]).unwrap_or_default());
                }
                k += 1;
            }
        }
    }
    out
}

/// Titles and options for the conflict editor.
pub struct ConflictArgs {
    pub merged: PathBuf,
    pub theirs_title: String,
    pub mine_title: String,
    pub merged_title: String,
    /// Temp files to delete on close (`/deletebasetheirsmineonclose`).
    pub cleanup: Vec<PathBuf>,
}

/// Three-way conflict editor on a file with conflict markers. After saving it
/// offers to mark the file resolved (`/command:resolve /noquestion /silent`).
pub fn run_conflict(args: ConflictArgs) -> DialogResult<i32> {
    let text = read_text(&args.merged);
    let segments = Rc::new(conflict::parse(&text));
    let count = segments
        .iter()
        .filter(|s| matches!(s, Segment::Conflict(_)))
        .count();
    let dlg = ConflictViewer::new()?;
    set_style!(dlg);
    dlg.set_dialog_title(format!("{} - KoopaGitMerge", args.merged_title).into());
    dlg.set_left_title(args.theirs_title.clone().into());
    dlg.set_right_title(args.mine_title.clone().into());
    dlg.set_merged_title(args.merged_title.clone().into());
    let choices = Rc::new(RefCell::new(vec![Choice::Unresolved; count]));
    let model = Rc::new(VecModel::<ConflictRow>::default());
    dlg.set_rows(ModelRc::from(model.clone()));

    let refresh = {
        let weak = dlg.as_weak();
        let segments = segments.clone();
        let choices = choices.clone();
        let model = model.clone();
        move || {
            let Some(d) = weak.upgrade() else { return };
            let ch = choices.borrow();
            model.set_vec(conflict_rows(&segments, &ch));
            d.set_merged_text(merged_preview(&segments, &ch).into());
            let open = ch.iter().filter(|c| **c == Choice::Unresolved).count();
            d.set_status_text(
                if count == 0 {
                    "No conflict markers found in this file.".to_owned()
                } else {
                    format!("{count} conflict(s), {open} unresolved")
                }
                .into(),
            );
            d.set_can_save(count > 0 && open == 0);
            d.set_modified(open < count);
        }
    };
    refresh();

    let header_row = {
        let segments = segments.clone();
        let choices = choices.clone();
        move |block: usize| {
            conflict_rows(&segments, &choices.borrow())
                .iter()
                .position(|r| r.kind == 2 && r.block == block as i32)
        }
    };
    let go = {
        let weak = dlg.as_weak();
        let header_row = header_row.clone();
        move |block: usize| {
            if let Some(d) = weak.upgrade() {
                d.set_current_block(block as i32);
                if let Some(r) = header_row(block) {
                    d.set_scroll_y(-(r.saturating_sub(2) as f32) * row_height());
                }
            }
        }
    };
    {
        let weak = dlg.as_weak();
        let go = go.clone();
        dlg.on_next_conflict(move || {
            let Some(d) = weak.upgrade() else { return };
            let next = (d.get_current_block() + 1).max(0) as usize;
            if next < count {
                go(next);
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let go = go.clone();
        dlg.on_previous_conflict(move || {
            let Some(d) = weak.upgrade() else { return };
            let cur = d.get_current_block();
            if cur > 0 {
                go((cur - 1) as usize);
            }
        });
    }
    {
        let choices = choices.clone();
        let refresh = refresh.clone();
        let go = go.clone();
        dlg.on_choose(move |block, which| {
            let Ok(b) = usize::try_from(block) else {
                return;
            };
            {
                let mut ch = choices.borrow_mut();
                let Some(slot) = ch.get_mut(b) else { return };
                *slot = match which {
                    1 => Choice::Mine,
                    2 => Choice::Theirs,
                    3 => Choice::MineThenTheirs,
                    4 => Choice::TheirsThenMine,
                    _ => Choice::Unresolved,
                };
            }
            refresh();
            // Move on to the next unresolved block.
            let next = choices
                .borrow()
                .iter()
                .enumerate()
                .skip(b + 1)
                .find(|(_, c)| **c == Choice::Unresolved)
                .map(|(i, _)| i);
            if let Some(n) = next {
                go(n);
            }
        });
    }
    // Three-pane block menus (spec-menus.md §8.3-§8.5): left = theirs,
    // right = mine, bottom = merged (acts on the current block).
    let block_of = {
        let model = model.clone();
        let weak = dlg.as_weak();
        move |row: i32, pane: i32| -> Option<usize> {
            use slint::Model;
            let b = if pane == 2 {
                weak.upgrade()?.get_current_block()
            } else {
                model.row_data(usize::try_from(row).ok()?)?.block
            };
            usize::try_from(b).ok()
        }
    };
    {
        let block_of = block_of.clone();
        dlg.on_pane_menu(move |row, pane| {
            let p = match pane {
                0 => Pane::Left,
                1 => Pane::Right,
                _ => Pane::Bottom,
            };
            crate::menus::to_model(&vmenu::three_pane(p, block_of(row, pane).is_some()))
        });
    }
    {
        let weak = dlg.as_weak();
        let segments = segments.clone();
        dlg.on_pane_menu_action(move |row, pane, id| {
            let Some(d) = weak.upgrade() else { return };
            let Some(action) = <ViewAction as crate::menus::ActionId>::from_id(id) else {
                return;
            };
            let block = block_of(row, pane);
            let choose = |which: i32| {
                if let Some(b) = block {
                    d.invoke_choose(b as i32, which);
                }
            };
            match action {
                ViewAction::UseTheirBlock => choose(2),
                ViewAction::UseMyBlock => choose(1),
                ViewAction::UseMineBeforeTheirs => choose(3),
                ViewAction::UseTheirsBeforeMine => choose(4),
                ViewAction::UseTheirFile | ViewAction::UseMyFile => {
                    let which = if action == ViewAction::UseTheirFile {
                        2
                    } else {
                        1
                    };
                    for b in 0..count {
                        d.invoke_choose(b as i32, which);
                    }
                }
                ViewAction::Copy => {
                    let conflict = segments
                        .iter()
                        .filter_map(|s| match s {
                            Segment::Conflict(c) => Some(c),
                            _ => None,
                        })
                        .nth(block.unwrap_or(usize::MAX));
                    if let Some(c) = conflict {
                        let lines = if pane == 0 { &c.theirs } else { &c.ours };
                        let _ = crate::clipboard::copy(&lines.concat());
                    }
                }
                _ => {}
            }
        });
    }
    let saved = Rc::new(std::cell::Cell::new(false));
    // What the notice bar asks; `closing`: Save was chosen on exit, so the
    // window closes once the resolve question is answered.
    let ask = Rc::new(std::cell::Cell::new(ConflictAsk::None));
    let closing = Rc::new(std::cell::Cell::new(false));
    {
        let weak = dlg.as_weak();
        let segments = segments.clone();
        let choices = choices.clone();
        let merged = args.merged.clone();
        let saved = saved.clone();
        let ask = ask.clone();
        let closing = closing.clone();
        dlg.on_save(move || {
            let Some(d) = weak.upgrade() else { return };
            let Some(text) = conflict::render(&segments, &choices.borrow()) else {
                return;
            };
            match std::fs::write(&merged, text) {
                Ok(()) => {
                    saved.set(true);
                    d.set_modified(false);
                    d.set_notice_error(false);
                    d.set_notice_text(
                        format!(
                            "{} saved. Mark the file as resolved?",
                            merged.file_name().unwrap_or_default().to_string_lossy()
                        )
                        .into(),
                    );
                    d.set_notice_buttons(ModelRc::new(VecModel::from(vec![
                        SharedString::from("Mark as resolved"),
                        SharedString::from("Not now"),
                    ])));
                    ask.set(ConflictAsk::Resolve);
                }
                Err(e) => {
                    closing.set(false);
                    ask.set(ConflictAsk::None);
                    d.set_notice_error(true);
                    d.set_notice_text(format!("Saving failed: {e}").into());
                    d.set_notice_buttons(ModelRc::default());
                }
            }
        });
    }
    // Exit: with unsaved choices ask Save / Quit / Cancel (`CheckForSave`).
    let exit_choices = save_prompt(false, true);
    let request_close: Rc<dyn Fn() -> bool> = {
        let weak = dlg.as_weak();
        let ask = ask.clone();
        let exit_choices = exit_choices.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else {
                return true;
            };
            if !d.get_modified() {
                let _ = d.hide();
                return true;
            }
            ask.set(ConflictAsk::Exit);
            d.set_notice_error(false);
            d.set_notice_text(SAVE_PROMPT.into());
            d.set_notice_buttons(notice_buttons(&exit_choices));
            false
        })
    };
    {
        let weak = dlg.as_weak();
        let merged = args.merged.clone();
        dlg.on_notice_clicked(move |i| {
            let Some(d) = weak.upgrade() else { return };
            d.set_notice_text(SharedString::new());
            match ask.replace(ConflictAsk::None) {
                ConflictAsk::Resolve if i == 0 => {
                    spawn_launcher(
                        &Invocation::new("resolve")
                            .with_paths([merged.clone()])
                            .with("closeonend", "1")
                            .with_switch("noquestion")
                            .with_switch("silent"),
                    );
                    let _ = d.hide();
                }
                ConflictAsk::Resolve => {
                    if closing.get() {
                        let _ = d.hide();
                    }
                }
                ConflictAsk::Exit => match exit_choices.get(i as usize).map(|(_, c)| *c) {
                    Some(SaveChoice::Save) if !d.get_can_save() => {
                        // IDS_ERR_MAINFRAME_FILEHASCONFLICTS, without "save anyway".
                        d.set_notice_error(true);
                        d.set_notice_buttons(ModelRc::default());
                        d.set_notice_text(
                            "There are still unresolved conflicts!\nYou should resolve those conflicts first before saving."
                                .into(),
                        );
                    }
                    Some(SaveChoice::Save) => {
                        closing.set(true);
                        d.invoke_save();
                    }
                    Some(SaveChoice::Quit) => {
                        let _ = d.hide();
                    }
                    _ => {}
                },
                ConflictAsk::None => {}
            }
        });
    }
    {
        let request_close = request_close.clone();
        dlg.on_close_viewer(move || {
            request_close();
        });
    }
    dlg.window().on_close_requested(move || {
        if request_close() {
            slint::CloseRequestResponse::HideWindow
        } else {
            slint::CloseRequestResponse::KeepWindowShown
        }
    });
    if count > 0 {
        go(0);
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    for f in &args.cleanup {
        let _ = std::fs::remove_file(f);
    }
    Ok(if saved.get() { 0 } else { 1 })
}

// ---------------------------------------------------------------------------
// Unified diff

/// Line classification for the unified-diff viewer.
pub fn udiff_kind(line: &str) -> i32 {
    if line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
        || line.starts_with("new file mode")
        || line.starts_with("deleted file mode")
        || line.starts_with("similarity index")
        || line.starts_with("rename ")
    {
        4
    } else if line.starts_with("@@") {
        3
    } else if line.starts_with('+') {
        1
    } else if line.starts_with('-') {
        2
    } else {
        0
    }
}

/// Shows a unified diff (TortoiseGitUDiff).
pub fn run_udiff(title: &str, text: &str) -> DialogResult {
    let dlg = UDiffViewer::new()?;
    set_style!(dlg);
    dlg.set_dialog_title(format!("{title} - KoopaGitUDiff").into());
    let lines: Vec<UDiffLine> = text
        .lines()
        .map(|l| UDiffLine {
            text: display(l).into(),
            kind: udiff_kind(l),
        })
        .collect();
    let (adds, dels) = text.lines().fold((0, 0), |(a, d), l| match udiff_kind(l) {
        1 => (a + 1, d),
        2 => (a, d + 1),
        _ => (a, d),
    });
    dlg.set_status_text(
        if lines.is_empty() {
            "No differences.".to_owned()
        } else {
            format!("{adds} line(s) added, {dels} line(s) removed")
        }
        .into(),
    );
    dlg.set_lines(ModelRc::new(VecModel::from(lines)));
    {
        let weak = dlg.as_weak();
        dlg.on_close_viewer(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Blame

pub fn blame_rows(blame: &Blame) -> Vec<BlameRow> {
    let mut rows = Vec::with_capacity(blame.lines.len());
    let mut shade = false;
    let mut prev: Option<&str> = None;
    for l in &blame.lines {
        let first = prev != Some(l.commit.as_str());
        if first && prev.is_some() {
            shade = !shade;
        }
        prev = Some(&l.commit);
        let c = blame.commits.get(&l.commit);
        rows.push(BlameRow {
            line_no: l.final_line.to_string().into(),
            commit: l.commit.chars().take(8).collect::<String>().into(),
            author: c.map(|c| c.author.clone()).unwrap_or_default().into(),
            date: c
                .map(|c| crate::format::short_datetime(c.author_time, c.author_offset))
                .unwrap_or_default()
                .into(),
            text: display(&l.content).into(),
            shade,
            first_of_group: first,
        });
    }
    rows
}

/// TortoiseGitBlame for a repository-relative path. Double-click opens the log
/// at the line's commit.
pub fn run_blame(repo: &Repo, rel: &str, rev: Option<&str>, line: Option<usize>) -> DialogResult {
    let blame = repo.blame(
        rel,
        &BlameOptions {
            rev: rev.map(str::to_owned),
            ..Default::default()
        },
    )?;
    let dlg = BlameViewer::new()?;
    set_style!(dlg);
    let abs = repo.workdir().join(rel);
    dlg.set_dialog_title(
        format!(
            "{} - {} - KoopaGitBlame",
            abs.display(),
            rev.unwrap_or("Working Tree")
        )
        .into(),
    );
    dlg.set_rows(ModelRc::new(VecModel::from(blame_rows(&blame))));
    dlg.set_status_text(format!("{} line(s)", blame.lines.len()).into());
    let blame = Rc::new(blame);
    {
        let weak = dlg.as_weak();
        let blame = blame.clone();
        dlg.on_selected(move |i| {
            let Some(d) = weak.upgrade() else { return };
            if let Some(l) = blame.lines.get(i as usize)
                && let Some(c) = blame.commits.get(&l.commit)
            {
                d.set_status_text(
                    format!(
                        "{}  {} <{}>  {}  {}",
                        &c.id[..c.id.len().min(8)],
                        c.author,
                        c.author_mail,
                        crate::format::short_datetime(c.author_time, c.author_offset),
                        c.summary
                    )
                    .into(),
                );
            }
        });
    }
    {
        let blame = blame.clone();
        let abs = abs.clone();
        dlg.on_activated(move |i| {
            if let Some(l) = blame.lines.get(i as usize) {
                if l.commit.bytes().all(|b| b == b'0') {
                    return; // "Not Committed Yet"
                }
                spawn_launcher(
                    &Invocation::new("log")
                        .with_paths([abs.clone()])
                        .with("rev", l.commit.clone()),
                );
            }
        });
    }
    // Line menu (spec-menus.md §9.1).
    let previous = {
        let blame = blame.clone();
        let workdir = repo.workdir().to_owned();
        let rel = rel.to_owned();
        move |i: i32| -> Vec<koopagit_core::blame::PreviousFile> {
            let Some(l) = usize::try_from(i).ok().and_then(|i| blame.lines.get(i)) else {
                return Vec::new();
            };
            if l.commit.bytes().all(|b| b == b'0') {
                return Vec::new();
            }
            let file = blame
                .commits
                .get(&l.commit)
                .map(|c| c.filename.clone())
                .filter(|f| !f.is_empty())
                .unwrap_or_else(|| rel.clone());
            Repo::discover(&workdir)
                .and_then(|r| r.previous_files(&l.commit, &file))
                .unwrap_or_default()
        }
    };
    {
        let previous = previous.clone();
        let cache = Rc::new(crate::menus::MenuCache::default());
        dlg.on_line_menu(move |i| {
            let previous = previous.clone();
            cache.get(i, move || vmenu::blame_line(previous(i).len()))
        });
    }
    {
        let blame = blame.clone();
        let workdir = repo.workdir().to_owned();
        let abs = abs.clone();
        dlg.on_line_menu_action(move |i, id| {
            let Some((action, parent)) = vmenu::decode_blame(id) else {
                return;
            };
            let Some(l) = usize::try_from(i).ok().and_then(|i| blame.lines.get(i)) else {
                return;
            };
            let sha = l.commit.clone();
            let commit = blame.commits.get(&sha);
            let file = commit
                .map(|c| workdir.join(&c.filename))
                .unwrap_or_else(|| abs.clone());
            let prev = previous(i);
            let target = prev.get(parent.saturating_sub(1));
            match action {
                vmenu::BlameAction::BlamePrevious => {
                    if let Some(p) = target {
                        spawn_launcher(
                            &Invocation::new("blame")
                                .with_paths([workdir.join(&p.path)])
                                .with("endrev", p.parent.clone())
                                .with("line", l.orig_line.to_string()),
                        );
                    }
                }
                vmenu::BlameAction::CompareWithPrevious => {
                    if let Some(p) = target {
                        spawn_launcher(
                            &Invocation::new("diff")
                                .with_paths([workdir.join(&p.path)])
                                .with("startrev", p.parent.clone())
                                .with("endrev", sha),
                        );
                    }
                }
                vmenu::BlameAction::ShowLog => spawn_launcher(
                    &Invocation::new("log")
                        .with_paths([file])
                        .with("rev", sha.clone())
                        .with("endrev", sha),
                ),
                vmenu::BlameAction::CopySha => {
                    let _ = crate::clipboard::copy(&sha);
                }
                vmenu::BlameAction::CopyLog => {
                    let text = koopagit_core::git_cli::Git::new(&workdir)
                        .run([
                            "log",
                            "-1",
                            "--format=Revision: %H%nAuthor: %an <%ae>%nDate: %ad%nMessage:%n%B",
                            &sha,
                        ])
                        .unwrap_or_default()
                        .replace('\n', "\r\n");
                    let _ = crate::clipboard::copy(text.trim_end());
                }
            }
        });
    }
    {
        let weak = dlg.as_weak();
        let blame = blame.clone();
        dlg.on_copy(move || {
            let Some(d) = weak.upgrade() else { return };
            if let Some(l) = usize::try_from(d.get_current())
                .ok()
                .and_then(|i| blame.lines.get(i))
            {
                let _ = crate::clipboard::copy(l.content.trim_end_matches(['\r', '\n']));
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_close_viewer(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }
    if let Some(l) = line {
        let row = l.saturating_sub(1).min(blame.lines.len().saturating_sub(1));
        dlg.set_current(row as i32);
    }
    crate::window::prepare(&dlg);
    dlg.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_question_choices() {
        use SaveChoice as C;
        let labels =
            |l: bool, r: bool| -> Vec<&str> { save_prompt(l, r).iter().map(|(t, _)| *t).collect() };
        assert!(save_prompt(false, false).is_empty());
        assert_eq!(
            labels(true, true),
            [
                "Save Left File",
                "Save Right File",
                "Save all",
                "Quit",
                "Cancel"
            ]
        );
        assert_eq!(labels(true, false), ["Save Left File", "Quit", "Cancel"]);
        assert_eq!(labels(false, true), ["Save", "Quit", "Cancel"]);
        assert_eq!(save_prompt(false, true)[0].1, C::Save);
        assert_eq!(save_prompt(true, true)[2].1, C::SaveAll);
    }

    #[test]
    fn diff_row_texts() {
        let a = diff::split_lines("x\n\ty\n");
        let b = diff::split_lines("x\nz\nw\n");
        let rows = diff::align(&diff::diff_lines(&a, &b, &DiffOptions::default()));
        let r = diff_rows(&a, &b, &rows);
        assert_eq!(r.len(), 3);
        assert_eq!(
            (r[1].ltext.as_str(), r[1].rtext.as_str(), r[1].kind),
            ("    y", "z", 3)
        );
        assert_eq!(
            (r[2].lno.as_str(), r[2].rno.as_str(), r[2].kind),
            ("", "3", 2)
        );
    }

    #[test]
    fn conflict_rows_and_preview() {
        let segs = conflict::parse("a\n<<<<<<< HEAD\nm1\nm2\n=======\nt\n>>>>>>> b\nz\n");
        let rows = conflict_rows(&segs, &[Choice::Unresolved]);
        assert_eq!(
            rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
            [0, 2, 1, 1, 0]
        );
        assert_eq!((rows[3].ltext.as_str(), rows[3].rtext.as_str()), ("", "m2"));
        assert!(merged_preview(&segs, &[Choice::Unresolved]).contains("<<<<<<< HEAD"));
        assert_eq!(merged_preview(&segs, &[Choice::Theirs]), "a\nt\nz\n");
    }

    #[test]
    fn udiff_classes_and_blame_groups() {
        assert_eq!(
            ["diff --git a b", "@@ -1 +1 @@", "+x", "-y", " z"].map(udiff_kind),
            [4, 3, 1, 2, 0]
        );
        let sha = |c: char| c.to_string().repeat(40);
        let out = format!(
            "{a} 1 1 2\nauthor A\nauthor-time 0\nsummary s\n\tl1\n{a} 2 2\n\tl2\n{b} 3 3 1\nauthor B\nauthor-time 0\nsummary t\n\tl3\n",
            a = sha('a'),
            b = sha('b')
        );
        let rows = blame_rows(&koopagit_core::blame::parse_porcelain(&out));
        assert_eq!(
            rows.iter().map(|r| r.first_of_group).collect::<Vec<_>>(),
            [true, false, true]
        );
        assert_eq!(
            rows.iter().map(|r| r.shade).collect::<Vec<_>>(),
            [false, false, true]
        );
    }
}
