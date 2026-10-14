//! `/command:log` — `CLogDlg` ("Log Messages").

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::git_cli::Git;
use koopagit_core::history::{self, FilterFields, GraphRow, LogFilter, LogLimit};
use koopagit_core::launcher::Invocation;
use koopagit_core::log::{LogEntry, LogOptions, RefKind, Signature};
use koopagit_core::settings::{keys, workdir_key};
use koopagit_core::state::RepoState;
use koopagit_core::status::{StatusEntry, StatusOptions};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::filelist::{self, Item};
use crate::filelist_actions::{FileListEnv, FileListHost, FileListMenu, LateRefresh};
use crate::filelist_menu::PopMask;
use crate::log_menu::{self, LogAction, LogMask, LogMenuInput, MenuCommit, MenuRef};
use crate::logactions::{self, LogActions, full_refs, is_wc};
use crate::logtools::{self, JUMP_TYPES, RefMode};
use crate::menus::{Menu, MenuCache, to_model};
use crate::selection::{Mods, Selection};
use crate::{
    Context, DialogResult, FileListBridge, GraphPath, LogDialog, LogRow, MenuBlock, RefChip,
    StatRow, StatisticsDialog, keep_alive, spawn_launcher,
};

/// Lane width: three quarters of the row height, as upstream's `DrawGraph`.
const LANE: f32 = 16.0;
const ROW: f32 = 22.0;
/// Cubic Bézier control-point factor for a quarter ellipse.
const KAPPA: f32 = 0.552_284_8;
/// Number of branch line colours (`Lanes::COLORS_NUM`).
const LANE_COLORS: usize = 8;
/// `Colors\BranchLine1..8` defaults (`COLORREF`).
const BRANCH_LINE_DEFAULTS: [u32; LANE_COLORS] =
    [0, 255, 65280, 16711680, 8421504, 32896, 8421376, 8388736];

/// Path commands for one graph row (lane width 16, row height 22), grouped
/// by lane colour. A line takes the colour of the lane it occupies at the row
/// edge. As in upstream's `paintGraphLane`, a branch leaves or joins the node
/// along a horizontal line that bends into its lane with a quarter-ellipse
/// arc; a lane shifting sideways away from the node is an S-curve.
pub fn graph_paths(row: &GraphRow) -> Vec<(usize, String)> {
    let x = |lane: usize| lane as f32 * LANE + LANE / 2.0;
    let mid = ROW / 2.0;
    let mut paths: Vec<(usize, String)> = Vec::new();
    for l in &row.lines {
        let (edge, inner, ye) = if l.upper {
            (l.from, l.to, 0.0)
        } else {
            (l.to, l.from, ROW)
        };
        let (xe, xi) = (x(edge), x(inner));
        let cmd = if edge == inner {
            format!("M {xi} {mid} L {xe} {ye} ")
        } else if inner == row.node {
            let dir = if xe > xi { 1.0 } else { -1.0 };
            let xa = xe - dir * LANE / 2.0;
            format!(
                "M {xi} {mid} L {xa} {mid} C {} {mid} {xe} {} {xe} {ye} ",
                xa + dir * KAPPA * LANE / 2.0,
                ye + (mid - ye) * KAPPA,
            )
        } else {
            let yc = (mid + ye) / 2.0;
            format!("M {xi} {mid} C {xi} {yc} {xe} {yc} {xe} {ye} ")
        };
        let color = edge % LANE_COLORS;
        match paths.iter_mut().find(|(c, _)| *c == color) {
            Some((_, p)) => p.push_str(&cmd),
            None => paths.push((color, cmd)),
        }
    }
    paths
}

/// `COLORREF` (`0x00BBGGRR`) to RGB.
fn colorref_rgb(c: u32) -> [u8; 3] {
    [c as u8, (c >> 8) as u8, (c >> 16) as u8]
}

/// Dark-mode variant of a fixed colour: HSL lightness inverted, hue and
/// saturation kept (`CTheme::GetThemeColor(clr, true)`). Shifting every
/// channel by `255 - max - min` does exactly that.
fn invert_lightness(rgb: [u8; 3]) -> [u8; 3] {
    let max = i32::from(*rgb.iter().max().unwrap_or(&0));
    let min = i32::from(*rgb.iter().min().unwrap_or(&0));
    rgb.map(|v| (i32::from(v) + 255 - max - min).clamp(0, 255) as u8)
}

/// Applies the Colors 3 page (branch line colours, line width, node size).
fn apply_graph_settings(dlg: &LogDialog, settings: &koopagit_core::settings::Settings) {
    let rgb: Vec<[u8; 3]> = BRANCH_LINE_DEFAULTS
        .iter()
        .enumerate()
        .map(|(i, &d)| colorref_rgb(settings.dword(&format!(r"Colors\BranchLine{}", i + 1), d)))
        .collect();
    let model = |f: &dyn Fn([u8; 3]) -> [u8; 3]| {
        ModelRc::new(VecModel::from(
            rgb.iter()
                .map(|&c| {
                    let [r, g, b] = f(c);
                    slint::Color::from_rgb_u8(r, g, b)
                })
                .collect::<Vec<_>>(),
        ))
    };
    dlg.set_lane_colors(model(&|c| c));
    dlg.set_lane_colors_dark(model(&invert_lightness));
    let width = settings.dword(r"TortoiseProc\Graph\LogLineWidth", 2).max(1);
    let node = settings.dword(r"TortoiseProc\Graph\LogNodeSize", 10).max(1);
    dlg.set_graph_line_width(width as f32);
    // Upstream: r = laneWidth * LogNodeSize / 30, node diameter 2r.
    dlg.set_graph_node_size((2 * (LANE as u32 * node / 30)).max(2) as f32);
}

/// Graph column width for the widest row, between the header text and a cap.
fn graph_width(rows: &[GraphRow]) -> f32 {
    let lanes = rows.iter().map(|r| r.width).max().unwrap_or(0);
    (lanes as f32 * LANE).clamp(48.0, 320.0)
}

fn chip_kind(k: RefKind) -> i32 {
    match k {
        RefKind::CurrentBranch => 0,
        RefKind::LocalBranch => 1,
        RefKind::RemoteBranch => 2,
        RefKind::Tag => 3,
        RefKind::Stash => 4,
        RefKind::Other => 5,
    }
}

/// View ▸ Labels toggles.
#[derive(Debug, Clone, Copy)]
pub struct LabelFilter {
    pub tags: bool,
    pub local: bool,
    pub remote: bool,
    pub other: bool,
}

impl LabelFilter {
    pub fn shows(self, k: RefKind) -> bool {
        match k {
            RefKind::Tag => self.tags,
            RefKind::CurrentBranch | RefKind::LocalBranch => self.local,
            RefKind::RemoteBranch => self.remote,
            RefKind::Stash | RefKind::Other => self.other,
        }
    }
}

/// The text of the message pane.
pub fn details_text(e: &LogEntry, relative: bool, notes: Option<&str>) -> String {
    if is_wc(e) {
        return "Working tree changes\n".to_owned();
    }
    let date = format_date(e.author.time, e.author.offset, relative);
    let mut s = format!(
        "SHA-1: {}\nAuthor: {} <{}>\nDate: {date}\n",
        e.id, e.author.name, e.author.email
    );
    if e.committer.name != e.author.name || e.committer.email != e.author.email {
        s.push_str(&format!(
            "Committer: {} <{}>\n",
            e.committer.name, e.committer.email
        ));
    }
    if !e.parents.is_empty() {
        let short: Vec<&str> = e.parents.iter().map(|p| &p[..p.len().min(8)]).collect();
        s.push_str(&format!("Parent: {}\n", short.join(", ")));
    }
    s.push('\n');
    s.push_str(e.message.trim_end());
    s.push('\n');
    if let Some(n) = notes.filter(|n| !n.trim().is_empty()) {
        s.push_str("\nNotes:\n");
        s.push_str(n.trim_end());
        s.push('\n');
    }
    s
}

fn format_date(time: i64, offset: i32, relative: bool) -> String {
    if relative {
        crate::format::relative_time(time, crate::format::now())
    } else {
        crate::format::short_datetime(time, offset)
    }
}

/// The working-tree pseudo-row (`m_CommitHash` empty, parent HEAD).
fn wc_entry(head: &str, name: &str, email: &str) -> LogEntry {
    let sig = Signature {
        name: name.to_owned(),
        email: email.to_owned(),
        time: crate::format::now(),
        offset: 0,
    };
    LogEntry {
        id: String::new(),
        parents: vec![head.to_owned()],
        author: sig.clone(),
        committer: sig,
        subject: "Working tree changes".to_owned(),
        message: "Working tree changes".to_owned(),
        refs: Vec::new(),
    }
}

/// Per-commit changed paths and notes, fetched for the Paths/Notes filters.
type Extra = HashMap<String, (Vec<String>, String)>;

/// Loads changed paths and notes for `ids` with one `git log --no-walk --stdin`.
fn load_extra(workdir: &PathBuf, ids: &[String]) -> Extra {
    let mut out = Extra::new();
    if ids.is_empty() {
        return out;
    }
    let mut cmd = Git::new(workdir).command([
        "log",
        "--no-walk=unsorted",
        "--stdin",
        "--name-only",
        "--format=%x1e%H%x1f%N%x1d",
    ]);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
    let Ok(mut child) = cmd.spawn() else {
        return out;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(ids.join("\n").as_bytes());
    }
    let Ok(output) = child.wait_with_output() else {
        return out;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    for rec in text.split('\x1e').filter(|r| !r.trim().is_empty()) {
        let Some((id, rest)) = rec.split_once('\x1f') else {
            continue;
        };
        let (notes, names) = rest.split_once('\x1d').unwrap_or((rest, ""));
        let paths = names
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();
        out.insert(id.trim().to_owned(), (paths, notes.trim().to_owned()));
    }
    out
}

struct State {
    /// Loaded rows; index 0 is the working-tree row when present.
    entries: Vec<LogEntry>,
    /// Indices into `entries` currently shown (after the filter).
    visible: Vec<usize>,
    head: Option<String>,
    files: Vec<StatusEntry>,
    sel: Selection,
    /// Visible rows visited, for "Selection History" jumps.
    history: Vec<usize>,
    mode: RefMode,
    extra: Extra,
    repo_state: RepoState,
    highlight: Option<String>,
    /// Row whose menu was built with Shift held (not observable; always false).
    shift: bool,
}

impl State {
    fn entry(&self, row: usize) -> Option<&LogEntry> {
        self.visible.get(row).map(|&i| &self.entries[i])
    }

    fn selected_entries(&self) -> Vec<LogEntry> {
        self.sel
            .rows()
            .filter_map(|r| self.entry(r).cloned())
            .collect()
    }
}

/// What the Log dialog was opened with.
#[derive(Clone)]
struct Setup {
    workdir: PathBuf,
    rel: Vec<String>,
    range: Option<String>,
    limit: LogLimit,
    include_wc: bool,
    bare: bool,
    file_log: Option<PathBuf>,
}

pub fn run(ctx: &mut Context) -> DialogResult<i32> {
    let dlg = LogDialog::new()?;
    let repo = &ctx.repo;
    let inv = ctx.invocation.clone();
    let workdir = repo.workdir().to_owned();
    let mut settings = ctx.settings.clone();
    let wd_key = workdir_key(&workdir);

    let rel = ctx.rel_paths();
    let forced_whole = rel.is_empty();
    let whole_key = format!(r"TortoiseProc\ShowWholeProject\{wd_key}");
    let all_key = format!(r"LogDialog\AllBranch\{wd_key}");
    dlg.set_whole_project(forced_whole || settings.bool(&whole_key, false));
    dlg.set_whole_project_enabled(!forced_whole);

    let range = history::range_param(
        inv.get("range"),
        inv.get("startrev").or(inv.get("revstart")),
        inv.get("endrev").or(inv.get("revend")),
    );
    // AllBranch: 0 none, 1 all branches, 2 local branches, 3 basic refs.
    let all_default = settings.dword(&all_key, 0);
    let explicit_range = range.as_deref().is_some_and(|r| r != "HEAD");
    let mode = match (explicit_range, all_default) {
        (true, _) => RefMode::Head,
        (false, 1) => RefMode::All,
        (false, 2) => RefMode::LocalBranches,
        (false, 3) => RefMode::BasicRefs,
        _ => RefMode::Head,
    };
    dlg.set_all_branches(mode == RefMode::All);

    let now = crate::format::now();
    let limit = match inv.get("limit") {
        Some(v) => history::parse_limit_param(v, now),
        None => history::limit_from_scale(
            settings.dword(keys::NUMBER_OF_LOGS_SCALE, 0),
            settings.dword(keys::NUMBER_OF_LOGS, 1),
            now,
        ),
    };
    if let LogLimit::Since(t) = limit {
        dlg.set_date_from(logtools::format_ymd(t).into());
    }
    let relative = settings.bool("RelativeTimes", false);
    apply_graph_settings(&dlg, &settings);
    let diff_by_double_click = settings.bool("DiffByDoubleClickInLog", false);
    let labels_key = |k: &str| format!(r"LogDialog\{k}\{wd_key}");
    dlg.set_show_tags(settings.bool(&labels_key("ShowTags"), true));
    dlg.set_show_local_branches(settings.bool(&labels_key("ShowLocalBranches"), true));
    dlg.set_show_remote_branches(settings.bool(&labels_key("ShowRemoteBranches"), true));
    dlg.set_show_other_refs(settings.bool(&labels_key("ShowOtherRefs"), true));
    dlg.set_filter_regex(settings.bool("UseRegexFilter", false));
    dlg.set_filter_case(settings.bool("FilterCaseSensitively", false));
    dlg.set_jump_types(ModelRc::new(VecModel::from(
        JUMP_TYPES
            .iter()
            .map(|s| SharedString::from(*s))
            .collect::<Vec<_>>(),
    )));

    let branch = repo.current_branch()?;
    dlg.set_branch(branch.clone().unwrap_or_else(|| "HEAD".into()).into());
    dlg.set_ref_label(
        match &range {
            Some(r) if explicit_range => r.clone(),
            _ => mode.label(branch.as_deref()),
        }
        .into(),
    );
    let title_path = if dlg.get_whole_project() {
        workdir.clone()
    } else {
        ctx.title_path()
    };
    dlg.set_dialog_title(crate::window_title(&title_path, "Log Messages").into());

    if let Some(text) = inv.get("findstring") {
        dlg.set_filter_text(text.into());
    }

    let bare = repo.is_bare();
    let file_log = match (rel.as_slice(), ctx.paths.as_slice()) {
        ([_], [p]) if p.is_file() => Some(p.clone()),
        _ => None,
    };
    let setup = Setup {
        workdir: workdir.clone(),
        rel: rel.clone(),
        range: range.clone(),
        limit,
        include_wc: !bare && settings.bool("LogIncludeWorkingTreeChanges", true),
        bare,
        file_log: file_log.clone(),
    };

    let state = Rc::new(RefCell::new(State {
        entries: Vec::new(),
        visible: Vec::new(),
        head: repo.head_id()?,
        files: Vec::new(),
        sel: Selection::default(),
        history: Vec::new(),
        mode,
        extra: Extra::new(),
        repo_state: repo.state(),
        // `/rev` may be any revision expression; select the commit it names.
        highlight: inv.get("rev").and_then(|r| {
            repo.git()
                .run([
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{r}^{{commit}}"),
                ])
                .ok()
                .map(|id| id.trim().to_owned())
        }),
        shift: false,
    }));
    let menu_cache = Rc::new(MenuCache::default());
    let rows_model = Rc::new(VecModel::<LogRow>::default());
    dlg.set_rows(ModelRc::from(rows_model.clone()));
    let files_model = Rc::new(VecModel::<crate::FileRow>::default());
    dlg.set_files(ModelRc::from(files_model.clone()));

    // Changed-files pane: file-list context menu with the LogDlg mask. Its
    // refresh re-reads the files of the selected commit (bound below, once
    // `show_selection` exists).
    let files_refresh: LateRefresh = Rc::default();
    let files_menu = {
        let state = state.clone();
        let files_refresh = files_refresh.clone();
        let mut env = FileListEnv::working_tree(
            1,
            &workdir,
            PopMask::LOG_DLG,
            &crate::window_title(&workdir, "Log Messages"),
        );
        env.has_wc = !bare;
        FileListMenu::new(
            env,
            FileListHost {
                items: Box::new(move || {
                    state
                        .borrow()
                        .files
                        .iter()
                        .map(|f| Item {
                            entry: f.clone(),
                            checked: false,
                            is_dir: false,
                        })
                        .collect()
                }),
                model: files_model.clone(),
                refresh: Box::new(move || {
                    let f = files_refresh.borrow().clone();
                    if let Some(f) = f {
                        f();
                    }
                }),
            },
        )
    };
    FileListMenu::install(
        &dlg.global::<FileListBridge>(),
        std::slice::from_ref(&files_menu),
    );

    let ref_history_key = format!(r"History\LogRefs\{wd_key}");
    let ref_history = Rc::new(RefCell::new(settings.history(&ref_history_key, "ref")));

    // Rebuilds the visible rows from `entries` and the filter.
    let apply_filter = {
        let state = state.clone();
        let rows_model = rows_model.clone();
        let weak = dlg.as_weak();
        let menu_cache = menu_cache.clone();
        let workdir = workdir.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let filter = LogFilter {
                text: d.get_filter_text().to_string(),
                fields: FilterFields {
                    subject: d.get_filter_subject(),
                    messages: d.get_filter_messages(),
                    authors: d.get_filter_authors(),
                    emails: d.get_filter_emails(),
                    sha1: d.get_filter_sha(),
                    refnames: d.get_filter_refs(),
                    paths: d.get_filter_paths(),
                    notes: d.get_filter_notes(),
                },
                case_sensitive: d.get_filter_case(),
                regex: d.get_filter_regex(),
            };
            let filter_error = filter.compile().err();
            d.set_filter_error(filter_error.is_some());
            let labels = LabelFilter {
                tags: d.get_show_tags(),
                local: d.get_show_local_branches(),
                remote: d.get_show_remote_branches(),
                other: d.get_show_other_refs(),
            };
            let mut st = state.borrow_mut();
            if !filter.text.is_empty() && (filter.fields.paths || filter.fields.notes) {
                let missing: Vec<String> = st
                    .entries
                    .iter()
                    .filter(|e| !is_wc(e) && !st.extra.contains_key(&e.id))
                    .map(|e| e.id.clone())
                    .collect();
                let loaded = load_extra(&workdir, &missing);
                st.extra.extend(loaded);
            }
            let visible: Vec<usize> = st
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    if is_wc(e) {
                        return true;
                    }
                    let extra = st.extra.get(&e.id);
                    filter.matches_with(
                        e,
                        extra.map(|x| x.0.as_slice()),
                        extra.map(|x| x.1.as_str()),
                    )
                })
                .map(|(i, _)| i)
                .collect();
            let filtering = !filter.text.is_empty();
            let graph = if filtering {
                Vec::new()
            } else {
                history::graph(
                    visible
                        .iter()
                        .map(|&i| (st.entries[i].id.as_str(), st.entries[i].parents.as_slice())),
                )
            };
            d.set_graph_width(graph_width(&graph));
            let rows: Vec<LogRow> = visible
                .iter()
                .enumerate()
                .map(|(n, &i)| {
                    let e = &st.entries[i];
                    let g = graph.get(n).cloned().unwrap_or_default();
                    LogRow {
                        graph: ModelRc::new(VecModel::from(
                            graph_paths(&g)
                                .into_iter()
                                .map(|(color, commands)| GraphPath {
                                    commands: commands.into(),
                                    color: color as i32,
                                })
                                .collect::<Vec<_>>(),
                        )),
                        lanes: g.width as i32,
                        node: g.node as i32,
                        merge: e.parents.len() > 1,
                        refs: ModelRc::new(VecModel::from(
                            e.refs
                                .iter()
                                .filter(|r| labels.shows(r.kind))
                                .map(|r| RefChip {
                                    name: r.name.clone().into(),
                                    kind: chip_kind(r.kind),
                                })
                                .collect::<Vec<_>>(),
                        )),
                        message: e.subject.clone().into(),
                        // The working-tree row has no author or date.
                        author: if is_wc(e) {
                            SharedString::default()
                        } else {
                            e.author.name.clone().into()
                        },
                        date: if is_wc(e) {
                            SharedString::default()
                        } else {
                            format_date(e.author.time, e.author.offset, relative).into()
                        },
                        sha: e.id[..e.id.len().min(8)].to_string().into(),
                        is_head: st.head.as_deref() == Some(e.id.as_str()),
                        selected: false,
                        wc: is_wc(e),
                    }
                })
                .collect();
            rows_model.set_vec(rows);
            st.visible = visible;
            st.sel = Selection::default();
            menu_cache.invalidate();
            d.set_info_text(
                match filter_error {
                    Some(err) => format!("Invalid regular expression: {err}"),
                    None => info_text(&st),
                }
                .into(),
            );
        })
    };

    // Reflects the selection in the model and shows the current row's details.
    let show_selection = {
        let state = state.clone();
        let files_model = files_model.clone();
        let rows_model = rows_model.clone();
        let weak = dlg.as_weak();
        let workdir = workdir.clone();
        let menu_cache = menu_cache.clone();
        let files_menu = files_menu.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            menu_cache.invalidate();
            let mut st = state.borrow_mut();
            for r in 0..rows_model.row_count() {
                let want = st.sel.contains(r);
                if let Some(mut row) = rows_model.row_data(r) {
                    if row.selected != want {
                        row.selected = want;
                        rows_model.set_row_data(r, row);
                    }
                }
            }
            let current = st.sel.current();
            d.set_current_row(current.map_or(-1, |c| c as i32));
            if let Some(c) = current {
                if st.history.last() != Some(&c) {
                    st.history.push(c);
                }
            }
            let Some(e) = current.and_then(|c| st.entry(c).cloned()) else {
                d.set_details("".into());
                files_model.set_vec(Vec::new());
                st.files.clear();
                d.set_info_text(info_text(&st).into());
                files_menu.set_current_version(None);
                files_menu.after_rebuild();
                return;
            };
            let repo = Repo::discover_including_bare(&workdir).ok();
            let notes = if is_wc(&e) {
                None
            } else {
                Git::new(&workdir).run(["notes", "show", &e.id]).ok()
            };
            d.set_details(details_text(&e, relative, notes.as_deref()).into());
            let (files, numstat) = match &repo {
                Some(r) if is_wc(&e) => (
                    r.status(StatusOptions::default(), &[]).unwrap_or_default(),
                    r.numstat("HEAD"),
                ),
                Some(r) => (
                    r.commit_changes(&e.id).unwrap_or_default(),
                    r.commit_numstat(&e.id),
                ),
                None => (Vec::new(), HashMap::new()),
            };
            let rows: Vec<crate::FileRow> = files
                .iter()
                .map(|f| {
                    filelist::to_row(
                        &Item {
                            entry: f.clone(),
                            checked: false,
                            is_dir: false,
                        },
                        &numstat,
                    )
                })
                .collect();
            files_model.set_vec(rows);
            st.files = files;
            d.set_info_text(info_text(&st).into());
            drop(st);
            // The working-tree row lists working-tree changes; any other row
            // lists the commit's files against its parent.
            files_menu.set_current_version((!is_wc(&e)).then(|| e.id.clone()));
            files_menu.after_rebuild();
        })
    };
    *files_refresh.borrow_mut() = Some(show_selection.clone());

    // (Re)loads history in a background thread.
    let reload: Rc<dyn Fn()> = {
        let state = state.clone();
        let apply_filter = apply_filter.clone();
        let show_selection = show_selection.clone();
        let weak = dlg.as_weak();
        let setup = setup.clone();
        Rc::new(move || {
            let Some(d) = weak.upgrade() else { return };
            let mode = {
                let mut st = state.borrow_mut();
                if d.get_all_branches() {
                    st.mode = RefMode::All;
                } else if st.mode == RefMode::All {
                    st.mode = RefMode::Head;
                }
                st.mode.clone()
            };
            let (since, until) =
                logtools::date_range(d.get_date_from().as_str(), d.get_date_to().as_str());
            let range = match &mode {
                RefMode::Range(r) => Some(r.clone()),
                RefMode::Head => setup.range.clone(),
                _ => None,
            };
            let options = LogOptions {
                all_branches: mode == RefMode::All,
                first_parent: d.get_first_parent(),
                no_merges: d.get_no_merges(),
                paths: if d.get_whole_project() {
                    Vec::new()
                } else {
                    setup.rel.clone()
                },
                limit: match setup.limit {
                    LogLimit::Commits(n) => Some(n),
                    _ => None,
                },
                range,
                since,
                until,
                revs: mode.revs(),
            };
            let with_wc = setup.include_wc
                && matches!(mode, RefMode::Head | RefMode::All)
                && (setup.range.is_none() || matches!(mode, RefMode::All))
                && until.is_none();
            d.set_loading(true);
            let weak = weak.clone();
            let state = state.clone();
            let apply_filter = apply_filter.clone();
            let show_selection = show_selection.clone();
            let workdir = setup.workdir.clone();
            let token = LoadToken::register(move |result: LoadResult| {
                let Some(d) = weak.upgrade() else { return };
                d.set_loading(false);
                match result {
                    Ok((mut entries, head, repo_state, user)) => {
                        if with_wc {
                            if let Some(h) = &head {
                                entries.insert(0, wc_entry(h, &user.0, &user.1));
                            }
                        }
                        {
                            let mut st = state.borrow_mut();
                            st.entries = entries;
                            st.head = head;
                            st.repo_state = repo_state;
                        }
                        apply_filter();
                        let row = {
                            let st = state.borrow();
                            let target = st.highlight.clone().or(st.head.clone());
                            target
                                .and_then(|t| {
                                    st.visible.iter().position(|&i| {
                                        !st.entries[i].id.is_empty()
                                            && st.entries[i].id.starts_with(&t)
                                    })
                                })
                                .unwrap_or(0)
                        };
                        if !state.borrow().visible.is_empty() {
                            state.borrow_mut().sel.select_only(row);
                        }
                        show_selection();
                    }
                    Err(e) => {
                        d.set_notice_error(true);
                        d.set_notice_text(e.into());
                    }
                }
            });
            let weak_thread = d.as_weak();
            std::thread::spawn(move || {
                let result = Repo::discover_including_bare(&workdir)
                    .map_err(|e| e.to_string())
                    .and_then(|r| {
                        let head = r.head_id().ok().flatten();
                        let state = r.state();
                        let user = r.user_identity();
                        r.log(&options)
                            .map(|l| (l, head, state, user))
                            .map_err(|e| e.to_string())
                    });
                let _ = weak_thread.upgrade_in_event_loop(move |_| token.complete(result));
            });
        })
    };

    let actions = Rc::new(LogActions {
        workdir: workdir.clone(),
        file: file_log.clone(),
        log_path: ctx
            .paths
            .first()
            .cloned()
            .unwrap_or_else(|| workdir.clone()),
        relative_dates: relative,
        reload: reload.clone(),
        find: {
            let weak = dlg.as_weak();
            Rc::new(move || {
                if let Some(d) = weak.upgrade() {
                    d.invoke_focus_filter();
                }
            })
        },
        show_text: {
            let weak = dlg.as_weak();
            Rc::new(move |text: String| {
                if let Some(d) = weak.upgrade() {
                    d.set_copy_text(text.into());
                }
            })
        },
        reflog: false,
    });

    {
        let reload = reload.clone();
        dlg.on_refresh(move || reload());
    }
    {
        let apply_filter = apply_filter.clone();
        let show_selection = show_selection.clone();
        let state = state.clone();
        let mut settings = settings.clone();
        let weak = dlg.as_weak();
        dlg.on_filter_changed(move || {
            apply_filter();
            if !state.borrow().visible.is_empty() {
                state.borrow_mut().sel.select_only(0);
            }
            show_selection();
            if let Some(d) = weak.upgrade() {
                settings.set_bool("UseRegexFilter", d.get_filter_regex());
                settings.set_bool("FilterCaseSensitively", d.get_filter_case());
                let _ = settings.save();
            }
        });
    }
    {
        let apply_filter = apply_filter.clone();
        let show_selection = show_selection.clone();
        let state = state.clone();
        dlg.on_labels_changed(move || {
            let keep = state.borrow().sel.clone();
            apply_filter();
            state.borrow_mut().sel = keep;
            show_selection();
        });
    }
    {
        let state = state.clone();
        let show_selection = show_selection.clone();
        dlg.on_row_pressed(move |row, toggle, range| {
            state
                .borrow_mut()
                .sel
                .click(row as usize, Mods { toggle, range });
            show_selection();
        });
    }
    {
        let state = state.clone();
        let show_selection = show_selection.clone();
        dlg.on_key_move(move |delta, extend| {
            let len = state.borrow().visible.len();
            state.borrow_mut().sel.step(delta as isize, len, extend);
            show_selection();
        });
    }
    {
        let state = state.clone();
        let show_selection = show_selection.clone();
        dlg.on_select_all(move || {
            let len = state.borrow().visible.len();
            state.borrow_mut().sel.select_all(len);
            show_selection();
        });
    }
    {
        let state = state.clone();
        let workdir = workdir.clone();
        dlg.on_file_activated(move |i| {
            let st = state.borrow();
            let (Some(e), Some(f)) = (
                st.sel.current().and_then(|c| st.entry(c)),
                st.files.get(i as usize),
            ) else {
                return;
            };
            let inv = Invocation::new("diff").with_paths([workdir.join(&f.path)]);
            let inv = if is_wc(e) {
                inv
            } else {
                inv.with("startrev", logactions::parent(e, 1))
                    .with("endrev", e.id.clone())
            };
            spawn_launcher(&inv);
        });
    }

    // Row context menu: right-clicking an unselected row selects only it.
    {
        let state = state.clone();
        let menu_cache = menu_cache.clone();
        let show_selection = show_selection.clone();
        let setup = setup.clone();
        dlg.on_row_menu(move |row| -> ModelRc<MenuBlock> {
            let r = row as usize;
            let changed = {
                let mut st = state.borrow_mut();
                if st.sel.contains(r) {
                    false
                } else {
                    st.sel.right_click(r);
                    true
                }
            };
            if changed {
                menu_cache.invalidate();
                // Update the highlight after the menu has been built.
                let show = show_selection.clone();
                slint::Timer::single_shot(std::time::Duration::ZERO, move || show());
            }
            let st = state.borrow();
            menu_cache.get(row, || log_menu::build(&menu_input(&st, &setup)))
        });
    }
    {
        let state = state.clone();
        let actions = actions.clone();
        dlg.on_menu_action(move |_row, id| {
            let Some((action, k)) = log_menu::decode(id) else {
                return;
            };
            let sel = state.borrow().selected_entries();
            actions.run(action, k, &sel);
        });
    }
    {
        let state = state.clone();
        let actions = actions.clone();
        dlg.on_row_activated(move |row| {
            if !diff_by_double_click {
                return;
            }
            let e = state.borrow().entry(row as usize).cloned();
            if let Some(e) = e {
                actions.run(LogAction::CompareWithPrevious, 1, &[e]);
            }
        });
    }
    // Keyboard (LogDlg.cpp PreTranslateMessage): Ctrl+C copies the selected
    // commits, Ctrl+V jumps to a pasted hash, Escape closes without OK.
    {
        let state = state.clone();
        let actions = actions.clone();
        dlg.on_copy_selection(move || {
            let sel = state.borrow().selected_entries();
            actions.run(LogAction::CopyFull, 0, &sel);
        });
    }
    {
        let state = state.clone();
        let show_selection = show_selection.clone();
        let weak = dlg.as_weak();
        let min_len = settings.dword("ShortHashLengthForHyperLinkInLogMessage", 7) as usize;
        dlg.on_paste_hash(move || {
            let Some(text) = crate::clipboard::text() else {
                return;
            };
            let Some(prefix) = logtools::pasted_hash(&text, min_len) else {
                return;
            };
            let target = {
                let st = state.borrow();
                let Some(cur) = st.sel.current() else { return };
                let ids: Vec<&str> = st
                    .visible
                    .iter()
                    .map(|&i| st.entries[i].id.as_str())
                    .collect();
                logtools::find_hash(&ids, cur, prefix)
            };
            match target {
                Some(t) => {
                    state.borrow_mut().sel.select_only(t);
                    show_selection();
                }
                None => {
                    if let Some(d) = weak.upgrade() {
                        d.set_notice_error(false);
                        d.set_notice_text("No more revisions found.".into());
                        d.set_notice_buttons(ModelRc::new(VecModel::from(vec![
                            SharedString::from("OK"),
                        ])));
                    }
                }
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_cancel(move || {
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }

    // Ref label menu (LogDlg.cpp:1368-1456).
    {
        let ref_history = ref_history.clone();
        let workdir = workdir.clone();
        dlg.on_ref_menu(move || {
            let fetch_head = Git::new(&workdir)
                .output(["rev-parse", "--verify", "--quiet", "FETCH_HEAD"])
                .is_ok_and(|o| o.success());
            let branch = Repo::discover_including_bare(&workdir)
                .ok()
                .and_then(|r| r.current_branch().ok().flatten());
            let mut m = Menu::new();
            m.raw(1, "Browse References", true);
            m.raw(
                2,
                match &branch {
                    Some(b) => format!("HEAD -> \"{b}\""),
                    None => "HEAD".to_owned(),
                },
                true,
            );
            m.raw(3, "FETCH_HEAD", fetch_head);
            m.raw(4, "All", true);
            m.raw(5, "All basic refs", true);
            m.raw(6, "All local branches", true);
            m.sep();
            for (k, h) in ref_history.borrow().iter().enumerate() {
                let label = if h.chars().count() > 150 {
                    format!("{}...", h.chars().take(150).collect::<String>())
                } else {
                    h.clone()
                };
                m.raw(7 + k as i32, label, true);
            }
            to_model(&m.finish())
        });
    }
    {
        let state = state.clone();
        let reload = reload.clone();
        let weak = dlg.as_weak();
        let ref_history = ref_history.clone();
        let workdir = workdir.clone();
        let mut settings = settings.clone();
        let ref_history_key = ref_history_key.clone();
        dlg.on_ref_menu_action(move |id| {
            let Some(d) = weak.upgrade() else { return };
            let mode = match id {
                1 => {
                    spawn_launcher(&Invocation::new("refbrowse").with_paths([workdir.clone()]));
                    return;
                }
                2 => RefMode::Head,
                3 => RefMode::Range("FETCH_HEAD".into()),
                4 => RefMode::All,
                5 => RefMode::BasicRefs,
                6 => RefMode::LocalBranches,
                k => match ref_history.borrow().get((k - 7) as usize) {
                    Some(r) => RefMode::Range(r.clone()),
                    None => return,
                },
            };
            if let RefMode::Range(r) = &mode {
                settings.push_history(&ref_history_key, "ref", r, 5);
                let _ = settings.save();
                *ref_history.borrow_mut() = settings.history(&ref_history_key, "ref");
            }
            let branch = Repo::discover_including_bare(&workdir)
                .ok()
                .and_then(|r| r.current_branch().ok().flatten());
            d.set_ref_label(mode.label(branch.as_deref()).into());
            d.set_all_branches(mode == RefMode::All);
            state.borrow_mut().mode = mode;
            reload();
        });
    }
    {
        let workdir = workdir.clone();
        dlg.on_browse_refs(move || {
            spawn_launcher(&Invocation::new("refbrowse").with_paths([workdir.clone()]));
        });
    }

    // From date menu (LogDlg.cpp:1458-1533).
    {
        let scale = settings.dword(keys::NUMBER_OF_LOGS_SCALE, 0);
        let n = settings.dword(keys::NUMBER_OF_LOGS, 1);
        dlg.on_from_menu(move || {
            let mut m = Menu::new();
            m.raw(1, "No limitation", true);
            if scale >= 2 && n > 0 {
                let unit = match scale {
                    2 => "commit(s)",
                    3 => "year(s)",
                    4 => "month(s)",
                    _ => "week(s)",
                };
                m.sep();
                m.raw(2, format!("Last {n} {unit}"), true);
            }
            m.sep();
            m.raw(3, "Configure default", true);
            to_model(&m.finish())
        });
    }
    {
        let weak = dlg.as_weak();
        let reload = reload.clone();
        let workdir = workdir.clone();
        let scale = settings.dword(keys::NUMBER_OF_LOGS_SCALE, 0);
        let n = settings.dword(keys::NUMBER_OF_LOGS, 1);
        dlg.on_from_menu_action(move |id| {
            let Some(d) = weak.upgrade() else { return };
            match id {
                1 => {
                    d.set_date_from("".into());
                    d.set_date_to("".into());
                    reload();
                }
                2 => {
                    if let LogLimit::Since(t) =
                        history::limit_from_scale(scale, n, crate::format::now())
                    {
                        d.set_date_from(logtools::format_ymd(t).into());
                    }
                    reload();
                }
                3 => spawn_launcher(
                    &Invocation::new("settings")
                        .with_paths([workdir.clone()])
                        .with("page", "dialog"),
                ),
                _ => {}
            }
        });
    }

    // Message pane menu (LogDlg.cpp:1535-1590). Id 1 (Copy) is handled in Slint.
    {
        let state = state.clone();
        dlg.on_message_menu(move |has_selection| {
            let st = state.borrow();
            let real_commit = st.sel.len() == 1
                && st
                    .sel
                    .current()
                    .and_then(|c| st.entry(c))
                    .is_some_and(|e| !is_wc(e));
            let mut m = Menu::new();
            m.raw(1, "Cop&y", has_selection);
            m.sep();
            m.raw(2, "Copy all information to clipboard", true);
            if real_commit {
                m.sep();
                m.raw(3, "Edit &Notes", true);
            }
            to_model(&m.finish())
        });
    }
    {
        let state = state.clone();
        let actions = actions.clone();
        let weak = dlg.as_weak();
        dlg.on_message_menu_action(move |id| {
            let Some(d) = weak.upgrade() else { return };
            match id {
                2 => {
                    let text = d.get_details().to_string();
                    if !crate::clipboard::copy(&text) {
                        d.set_copy_text(text.into());
                    }
                }
                3 => {
                    let sel = state.borrow().selected_entries();
                    actions.run(LogAction::EditNote, 0, &sel);
                }
                _ => {}
            }
        });
    }

    // Jump bar.
    {
        let state = state.clone();
        let show_selection = show_selection.clone();
        let weak = dlg.as_weak();
        dlg.on_jump(move |up| {
            let Some(d) = weak.upgrade() else { return };
            let kind = d.get_jump_type().max(0) as usize;
            let text = d.get_jump_text().to_string().to_lowercase();
            let target = {
                let st = state.borrow();
                let Some(cur) = st.sel.current() else { return };
                let rows: Vec<&LogEntry> = st.visible.iter().map(|&i| &st.entries[i]).collect();
                if !text.is_empty() && kind <= 1 {
                    // Email jumps with text: search for that text instead.
                    let hit = |e: &LogEntry| {
                        let mail = if kind == 0 {
                            &e.author.email
                        } else {
                            &e.committer.email
                        };
                        mail.to_lowercase().contains(&text)
                    };
                    if up {
                        (0..cur).rev().find(|&i| hit(rows[i]))
                    } else {
                        (cur + 1..rows.len()).find(|&i| hit(rows[i]))
                    }
                } else {
                    let hist: Vec<usize> = st.history.clone();
                    logtools::jump(&rows, cur, kind, up, &hist)
                }
            };
            match target {
                Some(t) => {
                    state.borrow_mut().sel.select_only(t);
                    show_selection();
                }
                None => {
                    d.set_notice_error(false);
                    d.set_notice_text("No matching revision found.".into());
                    d.set_notice_buttons(ModelRc::new(VecModel::from(vec![SharedString::from(
                        "OK",
                    )])));
                }
            }
        });
    }
    {
        let weak = dlg.as_weak();
        dlg.on_notice_clicked(move |_| {
            if let Some(d) = weak.upgrade() {
                d.set_notice_text("".into());
                d.set_notice_buttons(ModelRc::default());
            }
        });
    }

    // Statistics.
    {
        let state = state.clone();
        let workdir = workdir.clone();
        dlg.on_statistics(move || {
            let (entries, ids): (Vec<LogEntry>, Vec<String>) = {
                let st = state.borrow();
                let entries: Vec<LogEntry> = st
                    .visible
                    .iter()
                    .map(|&i| st.entries[i].clone())
                    .filter(|e| !is_wc(e))
                    .collect();
                let ids = entries.iter().map(|e| e.id.clone()).collect();
                (entries, ids)
            };
            let extra = load_extra(&workdir, &ids);
            let files: HashSet<&String> = extra.values().flat_map(|x| x.0.iter()).collect();
            let refs: Vec<&LogEntry> = entries.iter().collect();
            let s = logtools::statistics(&refs, files.len());
            if let Err(e) = show_statistics(&workdir, &s) {
                eprintln!("koopagit: {e}");
            }
        });
    }
    {
        let weak = dlg.as_weak();
        // `/outfile:<file>`: the dialog is a commit picker; OK writes the
        // selected commit's SHA-1 to the file (`LogCommand.cpp`).
        let outfile = ctx.invocation.get("outfile").map(PathBuf::from);
        let state = state.clone();
        dlg.on_ok(move || {
            if let Some(file) = &outfile {
                let st = state.borrow();
                let picked = st
                    .sel
                    .current()
                    .into_iter()
                    .chain(st.sel.rows())
                    .filter_map(|r| st.entry(r))
                    .map(|e| e.id.clone())
                    .find(|id| !id.is_empty() && id.chars().any(|c| c != '0'));
                if let Some(id) = picked {
                    if let Err(e) = std::fs::write(file, format!("{id}\n")) {
                        eprintln!("koopagit: {e}");
                    }
                }
            }
            if let Some(d) = weak.upgrade() {
                let _ = d.hide();
            }
        });
    }

    reload();
    crate::snapshot::arm(&dlg);
    dlg.run()?;

    if !forced_whole {
        settings.set_bool(&whole_key, dlg.get_whole_project());
    }
    let mode = state.borrow().mode.clone();
    settings.set(
        &all_key,
        match mode {
            RefMode::All => 1u32,
            RefMode::LocalBranches => 2,
            RefMode::BasicRefs => 3,
            _ => 0,
        },
    );
    settings.set_bool(&labels_key("ShowTags"), dlg.get_show_tags());
    settings.set_bool(
        &labels_key("ShowLocalBranches"),
        dlg.get_show_local_branches(),
    );
    settings.set_bool(
        &labels_key("ShowRemoteBranches"),
        dlg.get_show_remote_branches(),
    );
    settings.set_bool(&labels_key("ShowOtherRefs"), dlg.get_show_other_refs());
    let _ = settings.save();
    Ok(0)
}

/// Builds the menu input for the current selection.
fn menu_input(st: &State, setup: &Setup) -> LogMenuInput {
    let rows: Vec<&LogEntry> = st.visible.iter().map(|&i| &st.entries[i]).collect();
    let mut inp = menu_input_for(
        &rows,
        &st.sel,
        st.head.as_deref(),
        &st.repo_state,
        LogMask::default_log(),
    );
    inp.has_wc = !setup.bare;
    inp.file_log = setup.file_log.is_some();
    inp.shift = st.shift;
    inp
}

/// The log-menu input for `sel` over the list `rows` (top to bottom). Shared
/// by the Log dialog and the Sync dialog's commit lists.
pub fn menu_input_for(
    rows: &[&LogEntry],
    sel: &Selection,
    head: Option<&str>,
    rs: &RepoState,
    mask: LogMask,
) -> LogMenuInput {
    let ids: HashMap<&str, &LogEntry> = rows
        .iter()
        .filter(|e| !is_wc(e))
        .map(|e| (e.id.as_str(), *e))
        .collect();
    let picked: Vec<usize> = sel.to_vec();
    let selected: Vec<MenuCommit> = picked
        .iter()
        .filter_map(|&r| rows.get(r))
        .map(|e| MenuCommit {
            id: e.id.clone(),
            parent_labels: e
                .parents
                .iter()
                .enumerate()
                .map(|(k, p)| {
                    let subject = ids
                        .get(p.as_str())
                        .map(|e| e.subject.as_str())
                        .unwrap_or("");
                    log_menu::parent_label(k + 1, subject, p)
                })
                .collect(),
            refs: full_refs(e).into_iter().map(MenuRef::new).collect(),
            is_head: head == Some(e.id.as_str()),
            is_stash: e.refs.iter().any(|r| r.kind == RefKind::Stash),
            has_bisect_ref: e.refs.iter().any(|r| r.name.starts_with("refs/bisect/")),
            reflog_ref: None,
            rolled_up: false,
        })
        .collect();
    let parent_of: HashMap<&str, &str> = rows
        .iter()
        .filter_map(|e| e.parents.first().map(|p| (e.id.as_str(), p.as_str())))
        .collect();
    let sel_ids: Vec<&str> = selected.iter().map(|c| c.id.as_str()).collect();
    LogMenuInput {
        mask,
        contiguous: sel.is_contiguous(),
        bottom_selected: picked.last().is_some_and(|&r| r + 1 == rows.len()),
        combine_ok: head.is_some_and(|h| logtools::combine_ok(&sel_ids, &parent_of, h)),
        selected,
        current_branch: rs.branch.clone(),
        has_wc: true,
        merge_active: rs.merge_active,
        bisect_active: rs.bisect_active,
        stash_exists: rs.has_stash,
        has_submodules: rs.has_submodules,
        git_svn: rs.is_git_svn,
        file_log: false,
        cherry_pick_mode: false,
        rollup_possible: false,
        shift: false,
        reflog: false,
    }
}

/// `Showing %ld revision(s), from revision %s to revision %s - %ld revision(s)
/// selected, %ld file(s) selected`.
fn info_text(st: &State) -> String {
    let real: Vec<&LogEntry> = st
        .visible
        .iter()
        .map(|&i| &st.entries[i])
        .filter(|e| !is_wc(e))
        .collect();
    let short = |e: Option<&&LogEntry>| e.map(|e| e.id[..8].to_owned()).unwrap_or_default();
    format!(
        "Showing {} revision(s), from revision {} to revision {} - {} revision(s) selected, {} file(s) selected",
        real.len(),
        short(real.last()),
        short(real.first()),
        st.sel.len(),
        0
    )
}

fn show_statistics(workdir: &std::path::Path, s: &logtools::Statistics) -> DialogResult {
    let dlg = StatisticsDialog::new()?;
    dlg.set_dialog_title(crate::window_title(workdir, "Statistics").into());
    let row = |l: &str, v: String| StatRow {
        label: l.into(),
        value: v.into(),
    };
    dlg.set_summary(ModelRc::new(VecModel::from(vec![
        row("Number of weeks", s.weeks.to_string()),
        row("Number of authors", s.authors.to_string()),
        row("Total commits", s.commits.to_string()),
        row("Total file changes", s.files_changed.to_string()),
        row("Most active author", s.most_active_author.clone()),
        row("Commits/week (avg.)", s.avg_commits_per_week.to_string()),
        row("Commits/week (min.)", s.least_commits_per_week.to_string()),
        row("Commits/week (max.)", s.most_commits_per_week.to_string()),
    ])));
    dlg.set_per_author(ModelRc::new(VecModel::from(
        s.per_author
            .iter()
            .map(|(a, n)| row(a, n.to_string()))
            .collect::<Vec<_>>(),
    )));
    dlg.set_per_week(ModelRc::new(VecModel::from(
        s.per_week
            .iter()
            .map(|(w, n)| row(w, n.to_string()))
            .collect::<Vec<_>>(),
    )));
    let weak = dlg.as_weak();
    dlg.on_dismiss(move || {
        if let Some(d) = weak.upgrade() {
            let _ = d.hide();
        }
    });
    crate::snapshot::arm(&dlg);
    dlg.show()?;
    keep_alive(dlg);
    Ok(())
}

/// Routes a background result back to UI-thread-only closures.
struct LoadToken(u64);

type LoadResult = Result<(Vec<LogEntry>, Option<String>, RepoState, (String, String)), String>;
type LoadCallbacks = HashMap<u64, Box<dyn FnOnce(LoadResult)>>;

thread_local! {
    static LOAD_CALLBACKS: RefCell<LoadCallbacks> = RefCell::new(HashMap::new());
}

impl LoadToken {
    fn register(cb: impl FnOnce(LoadResult) + 'static) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        LOAD_CALLBACKS.with(|m| m.borrow_mut().insert(id, Box::new(cb)));
        LoadToken(id)
    }

    fn complete(self, result: LoadResult) {
        if let Some(cb) = LOAD_CALLBACKS.with(|m| m.borrow_mut().remove(&self.0)) {
            cb(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koopagit_core::history::GraphLine;
    use koopagit_core::log::RefLabel;

    #[test]
    fn graph_paths_curve_into_lanes() {
        let line = |from, to, upper| GraphLine { from, to, upper };
        // Node in lane 0 forking to lane 1, with lane 2 shifting into lane 1.
        let row = GraphRow {
            node: 0,
            lines: vec![line(0, 0, true), line(0, 1, false), line(2, 1, false)],
            width: 3,
        };
        let paths = graph_paths(&row);
        assert_eq!(paths[0], (0, "M 8 11 L 8 0 ".to_string()));
        // Horizontal to the lane boundary, then a quarter ellipse down into lane 1.
        assert_eq!(paths[1].0, 1);
        assert!(paths[1].1.starts_with("M 8 11 L 16 11 C 20.41"));
        assert!(paths[1].1.contains(" 24 15.92"));
        // Away from the node: an S-curve, joined into the same colour group.
        assert!(paths[1].1.ends_with("M 40 11 C 40 16.5 24 16.5 24 22 "));
        assert_eq!(paths.len(), 2);
        assert!(graph_paths(&GraphRow::default()).is_empty());
    }

    #[test]
    fn dark_lane_colours_invert_lightness() {
        assert_eq!(invert_lightness(colorref_rgb(0)), [255, 255, 255]);
        assert_eq!(invert_lightness(colorref_rgb(255)), [255, 0, 0]);
        assert_eq!(invert_lightness(colorref_rgb(32896)), [255, 255, 127]);
    }

    #[test]
    fn details_wc_and_labels() {
        let sig = Signature {
            name: "Ann".into(),
            email: "a@x".into(),
            time: 0,
            offset: 0,
        };
        let e = LogEntry {
            id: "0123456789abcdef".into(),
            parents: vec!["fedcba9876543210".into()],
            author: sig.clone(),
            committer: sig,
            subject: "Subject".into(),
            message: "Subject\n\nBody\n".into(),
            refs: vec![RefLabel {
                kind: RefKind::Tag,
                name: "v1".into(),
            }],
        };
        let d = details_text(&e, false, Some("Reviewed"));
        assert!(
            d.starts_with("SHA-1: 0123456789abcdef\nAuthor: Ann <a@x>\nDate: 1970-01-01 00:00\n")
        );
        assert!(d.contains("Parent: fedcba98\n"));
        assert!(d.ends_with("Body\n\nNotes:\nReviewed\n"));
        let wc = wc_entry("fedcba98", "Ann", "a@x");
        assert!(is_wc(&wc));
        assert_eq!(details_text(&wc, false, None), "Working tree changes\n");
        let f = LabelFilter {
            tags: false,
            local: true,
            remote: true,
            other: true,
        };
        assert!(!f.shows(RefKind::Tag));
        assert!(f.shows(RefKind::CurrentBranch));
    }

    #[test]
    fn extra_loader_reads_paths_and_notes() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path());
        git.run(["init", "-q", "-b", "main"]).unwrap();
        git.run(["config", "user.name", "T"]).unwrap();
        git.run(["config", "user.email", "t@x"]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        git.run(["add", "."]).unwrap();
        git.run(["commit", "-qm", "one"]).unwrap();
        git.run(["notes", "add", "-m", "Reviewed-by: X", "HEAD"])
            .unwrap();
        let id = git.run(["rev-parse", "HEAD"]).unwrap().trim().to_owned();
        let extra = load_extra(&dir.path().to_owned(), std::slice::from_ref(&id));
        let (paths, notes) = &extra[&id];
        assert_eq!(paths, &["a.txt"]);
        assert_eq!(notes, "Reviewed-by: X");
    }
}
