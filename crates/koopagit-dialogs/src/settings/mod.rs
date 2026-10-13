//! `/command:settings [/page:<id>]` — the TortoiseGit settings dialog (`CSettings`).

mod editor;
pub mod pages;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use koopagit_core::Repo;
use koopagit_core::gitconfig::{self, ConfigScope, GitConfig};
use koopagit_core::launcher::Invocation;
use koopagit_core::settings::{Settings, keys};
use koopagit_shell::menu::{bits, settings_entries};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use koopagit_core::hooks::{self, Hook, HookType};

use self::pages::{
    Action, ClearTarget, Env, HookField, Kind, Opt, Page, RemoteField, Special, Store, block_trust,
    block_trust_value, colorref_to_hex, hex_to_colorref, join_prog, split_prog,
};
use crate::{
    CheckRow, DialogResult, KvRow, OptRow, SettingsDialog, TreeRow, spawn_launcher, window_title,
};

/// Questions shown in the notice bar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    None,
    /// Confirm saving to a different level than the one being viewed.
    SaveTo,
    /// Disable tag fetching for a newly added remote?
    TagOpt,
    /// Overwrite an existing remote?
    Overwrite,
    /// Fetch the newly added remote?
    Fetch(String),
    /// Remove the selected remote?
    Remove(String),
}

/// View and edit state of a page with the config-source selector.
#[derive(Debug, Clone)]
struct GitPage {
    honor_project: bool,
    scope: ConfigScope,
    save_to: ConfigScope,
    values: HashMap<&'static str, Option<String>>,
    dirty: HashSet<&'static str>,
}

#[derive(Debug, Clone, Default)]
struct RemoteEdit {
    list: Vec<String>,
    selected: Option<usize>,
    name: String,
    url: String,
    push_url: String,
    putty_key: String,
    tags: usize,
    push_default: bool,
    prune: Option<bool>,
    dirty: bool,
}

struct State {
    pages: Vec<Page>,
    current: usize,
    settings: Settings,
    reg_dirty: bool,
    cfg: GitConfig,
    in_repo: bool,
    git: HashMap<usize, GitPage>,
    remote: RemoteEdit,
    /// Opened with `/page:gitremote`: no "fetch newly added remote?" prompt.
    no_fetch: bool,
    pending: Pending,
    /// Answers already given during the current save.
    tag_answer: Option<bool>,
    overwrite_confirmed: bool,
    save_to_confirmed: bool,
    /// Built-in/external radio of program settings chosen in this session; an
    /// empty path cannot encode "external" on its own.
    prog_external: HashMap<&'static str, bool>,
    /// Hook Scripts page: all hooks, the selected one and the editor fields.
    hooks: Vec<Hook>,
    hook_selected: Option<usize>,
    hook_edit: Hook,
    /// Proxy / SSH values changed: rewrite `http.proxy` / `core.sshCommand` on apply.
    proxy_dirty: bool,
    ssh_dirty: bool,
}

fn new_hook() -> Hook {
    Hook {
        kind: HookType::StartCommit,
        path: String::new(),
        command: String::new(),
        wait: true,
        show: true,
        enabled: true,
    }
}

fn hook_label(h: &Hook) -> String {
    format!(
        "{}{} | {} | {}",
        if h.enabled { "" } else { "(disabled) " },
        h.kind.label(),
        h.path,
        h.command
    )
}

fn scope_list(honor_project: bool) -> Vec<ConfigScope> {
    let mut v = vec![ConfigScope::Effective, ConfigScope::Local];
    if honor_project {
        v.push(ConfigScope::Project);
    }
    v.extend([ConfigScope::Global, ConfigScope::System]);
    v
}

impl State {
    fn new(
        repo: Option<&Repo>,
        settings: Settings,
        cfg: GitConfig,
        requested: Option<&str>,
    ) -> Self {
        let in_repo = repo.is_some();
        let env = Env {
            in_repo,
            git_version: cfg
                .run_git(&["--version".to_owned()])
                .map(|v| v.trim().trim_start_matches("git version ").to_owned())
                .unwrap_or_else(|_| "unknown".to_owned()),
            xdg_config_exists: gitconfig::xdg_config_file().is_some_and(|f| f.exists()),
            is_windows: cfg!(windows),
        };
        let pages = pages::pages(&env);
        let current = pages::default_page(&pages, requested, in_repo);
        let mut st = State {
            pages,
            current,
            settings,
            reg_dirty: false,
            cfg,
            in_repo,
            git: HashMap::new(),
            remote: RemoteEdit::default(),
            no_fetch: requested == Some("gitremote"),
            pending: Pending::None,
            tag_answer: None,
            overwrite_confirmed: false,
            save_to_confirmed: false,
            prog_external: HashMap::new(),
            hooks: Vec::new(),
            hook_selected: None,
            hook_edit: new_hook(),
            proxy_dirty: false,
            ssh_dirty: false,
        };
        st.hooks = hooks::parse(&st.settings.string(hooks::SETTINGS_KEY, ""));
        for i in 0..st.pages.len() {
            if let Some(honor) = st.pages[i].config_source {
                let save_to = if in_repo {
                    ConfigScope::Local
                } else {
                    ConfigScope::Global
                };
                st.git.insert(
                    i,
                    GitPage {
                        honor_project: honor,
                        scope: ConfigScope::Effective,
                        save_to,
                        values: HashMap::new(),
                        dirty: HashSet::new(),
                    },
                );
                st.load_git(i);
                // Git page: with no identity at all, start on the Global level.
                if st.pages[i].id == "gitconfig" {
                    let g = &st.git[&i];
                    let empty = |k| {
                        g.values
                            .get(k)
                            .cloned()
                            .flatten()
                            .unwrap_or_default()
                            .is_empty()
                    };
                    if empty("user.name") && empty("user.email") {
                        st.set_scope(i, ConfigScope::Global);
                    }
                }
            }
        }
        st.reload_remotes(None);
        st
    }

    fn page(&self) -> &Page {
        &self.pages[self.current]
    }

    fn load_git(&mut self, page: usize) {
        let Some(g) = self.git.get(&page) else { return };
        let (scope, honor) = (g.scope, g.honor_project);
        let mut values = HashMap::new();
        for opt in &self.pages[page].options {
            if let Store::Git(key) = opt.store {
                values.insert(key, self.cfg.get(scope, key, honor));
            }
        }
        let g = self.git.get_mut(&page).expect("git page");
        g.values = values;
        g.dirty.clear();
    }

    fn set_scope(&mut self, page: usize, scope: ConfigScope) {
        if let Some(g) = self.git.get_mut(&page) {
            g.scope = scope;
            if matches!(
                scope,
                ConfigScope::Local | ConfigScope::Project | ConfigScope::Global
            ) {
                g.save_to = scope;
            }
        }
        self.load_git(page);
    }

    fn save_to_list(&self, honor: bool) -> Vec<ConfigScope> {
        let mut v = Vec::new();
        if self.in_repo {
            v.push(ConfigScope::Local);
            if honor {
                v.push(ConfigScope::Project);
            }
        }
        v.push(ConfigScope::Global);
        v
    }

    fn menu_mask(&self, ext: bool) -> u64 {
        let (lo, hi, def) = if ext {
            (
                keys::CONTEXT_MENU_EXT_ENTRIES_LOW,
                keys::CONTEXT_MENU_EXT_ENTRIES_HIGH,
                bits::DEFAULT_EXT,
            )
        } else {
            (
                keys::CONTEXT_MENU_ENTRIES,
                keys::CONTEXT_MENU_ENTRIES_HIGH,
                bits::DEFAULT_TOP,
            )
        };
        u64::from(self.settings.dword(lo, def as u32))
            | (u64::from(self.settings.dword(hi, (def >> 32) as u32)) << 32)
    }

    fn set_menu_mask(&mut self, ext: bool, mask: u64) {
        let (lo, hi) = if ext {
            (
                keys::CONTEXT_MENU_EXT_ENTRIES_LOW,
                keys::CONTEXT_MENU_EXT_ENTRIES_HIGH,
            )
        } else {
            (keys::CONTEXT_MENU_ENTRIES, keys::CONTEXT_MENU_ENTRIES_HIGH)
        };
        self.settings.set(lo, mask as u32);
        self.settings.set(hi, (mask >> 32) as u32);
        self.reg_dirty = true;
    }

    fn checklist(&self) -> Vec<CheckRow> {
        let Special::Checklist(ext) = self.page().special else {
            return vec![];
        };
        let mask = self.menu_mask(ext);
        settings_entries()
            .into_iter()
            .map(|(bit, label)| CheckRow {
                label: label.into(),
                checked: mask & bit != 0,
            })
            .collect()
    }

    fn prog_is_external(&self, key: &'static str) -> bool {
        self.prog_external
            .get(key)
            .copied()
            .unwrap_or_else(|| split_prog(&self.reg(key, "")).0)
    }

    fn reg(&self, key: &str, default: &str) -> String {
        self.settings.string(key, default)
    }

    fn git_page(&self) -> Option<&GitPage> {
        self.git.get(&self.current)
    }

    fn clear_count(&self, t: ClearTarget) -> usize {
        match t {
            ClearTarget::UrlHistory => pages::URL_HISTORY_KEYS
                .iter()
                .map(|k| self.settings.count_prefix(k))
                .sum(),
            ClearTarget::LogMessages => self
                .settings
                .keys()
                .filter(|k| k.starts_with(r"History\commit"))
                .count(),
            ClearTarget::DialogSizes => self.settings.count_prefix(r"TortoiseProc\ResizableState"),
            ClearTarget::StoredDecisions => pages::STORED_DECISIONS
                .iter()
                .filter(|k| self.settings.get(k).is_some())
                .count(),
            ClearTarget::TempFiles => std::fs::read_dir(temp_dir())
                .map(|d| d.count())
                .unwrap_or(0),
            ClearTarget::LogCache | ClearTarget::Authentication => 0,
        }
    }

    fn clear(&mut self, t: ClearTarget) {
        match t {
            ClearTarget::UrlHistory => {
                for k in pages::URL_HISTORY_KEYS {
                    self.settings.remove_prefix(k);
                }
            }
            ClearTarget::LogMessages => {
                let keys: Vec<String> = self
                    .settings
                    .keys()
                    .filter(|k| k.starts_with(r"History\commit"))
                    .map(str::to_owned)
                    .collect();
                for k in keys {
                    self.settings.remove(&k);
                }
            }
            ClearTarget::DialogSizes => self.settings.remove_prefix(r"TortoiseProc\ResizableState"),
            ClearTarget::StoredDecisions => {
                for k in pages::STORED_DECISIONS {
                    self.settings.remove(k);
                }
            }
            ClearTarget::TempFiles => {
                let _ = std::fs::remove_dir_all(temp_dir());
            }
            ClearTarget::LogCache | ClearTarget::Authentication => {}
        }
        // Saved Data clears immediately, like upstream.
        let _ = self.settings.save();
    }

    /// The rows of the current page.
    fn rows(&self) -> Vec<OptRow> {
        let page = self.page();
        let git = self.git_page();
        let read_only =
            git.is_some_and(|g| matches!(g.scope, ConfigScope::Effective | ConfigScope::System));
        let cache_default = self.reg("CacheType", "1") == "1";
        page.options
            .iter()
            .map(|opt| self.row(opt, git, read_only, cache_default))
            .collect()
    }

    fn row(
        &self,
        opt: &Opt,
        git: Option<&GitPage>,
        read_only: bool,
        cache_default: bool,
    ) -> OptRow {
        let mut row = OptRow {
            kind: kind_code(&opt.kind),
            label: opt.label.clone().into(),
            text: SharedString::new(),
            checked: false,
            tri: 2,
            index: 0,
            choices: ModelRc::default(),
            enabled: !opt.needs_repo || self.in_repo,
            inherit_visible: false,
            inherited: false,
            swatch: slint::Color::from_argb_u8(0, 0, 0, 0),
        };
        match &opt.kind {
            Kind::Combo { labels, .. } | Kind::Radio { labels, .. } => {
                let v: Vec<SharedString> = labels.iter().map(|l| l.as_str().into()).collect();
                row.choices = ModelRc::new(VecModel::from(v));
            }
            Kind::Clear(t) => {
                let n = self.clear_count(*t);
                row.text = n.to_string().into();
                row.enabled = n > 0;
            }
            _ => {}
        }
        match &opt.store {
            Store::None => {}
            Store::Reg(key, default) => {
                let v = self.reg(key, default);
                if matches!(*key, "UnversionedAsModified" | "TGitCacheRecurseSubmodules") {
                    row.enabled &= cache_default;
                }
                if opt.kind == Kind::Color {
                    let c: u32 = v.parse().unwrap_or(0);
                    row.text = colorref_to_hex(c).into();
                    row.swatch = slint::Color::from_rgb_u8(
                        (c & 0xFF) as u8,
                        ((c >> 8) & 0xFF) as u8,
                        ((c >> 16) & 0xFF) as u8,
                    );
                } else {
                    fill_value(&mut row, &opt.kind, Some(&v), false);
                }
                if pages::PROXY_ENABLED != *key && key.contains("http-proxy-") {
                    row.enabled &= self.settings.bool(pages::PROXY_ENABLED, false);
                }
            }
            Store::Hook(field) => {
                let h = &self.hook_edit;
                match field {
                    HookField::Enabled => row.checked = h.enabled,
                    HookField::Type => {
                        row.index =
                            HookType::ALL.iter().position(|t| *t == h.kind).unwrap_or(0) as i32
                    }
                    HookField::ThisRepository => {
                        row.checked = self
                            .cfg
                            .workdir()
                            .is_some_and(|w| w.to_string_lossy() == h.path);
                    }
                    HookField::Path => row.text = h.path.as_str().into(),
                    HookField::Command => row.text = h.command.as_str().into(),
                    HookField::Wait => row.checked = h.wait,
                    HookField::Hide => row.checked = !h.show,
                }
            }
            Store::Git(key) => {
                let Some(g) = git else { return row };
                let v = g.values.get(key).cloned().flatten();
                row.enabled &= !read_only;
                row.inherit_visible = opt.inherit;
                row.inherited = v.is_none();
                let shown = match (key, &v) {
                    (&"core.autocrlf", Some(raw)) => {
                        Some(gitconfig::normalize_combo(raw, &["input"]))
                    }
                    (&"core.safecrlf", Some(raw)) => {
                        Some(gitconfig::normalize_combo(raw, &["warn"]))
                    }
                    _ => v.clone(),
                };
                fill_value(&mut row, &opt.kind, shown.as_deref(), true);
                if matches!(opt.kind, Kind::TriState)
                    && v.is_none()
                    && g.scope == ConfigScope::Effective
                {
                    // Git defaults when unset: core.quotepath true, fetch.prune false.
                    row.tri = i32::from(*key == "core.quotepath");
                }
            }
            Store::ProgMode(key) => row.index = i32::from(self.prog_is_external(key)),
            Store::ProgPath(key) => {
                row.text = split_prog(&self.reg(key, "")).1.into();
            }
            Store::BlockTrust(trust) => {
                let (block, t) = block_trust(self.settings.dword("MergeBlockTrustBehavior", 0));
                row.checked = if *trust { t } else { block };
                if *trust {
                    row.enabled &= block;
                }
            }
            Store::Remote(field) => {
                let r = &self.remote;
                match field {
                    RemoteField::Name => row.text = r.name.as_str().into(),
                    RemoteField::Url => row.text = r.url.as_str().into(),
                    RemoteField::PushUrl => row.text = r.push_url.as_str().into(),
                    RemoteField::PuttyKey => row.text = r.putty_key.as_str().into(),
                    RemoteField::Tags => row.index = r.tags as i32,
                    RemoteField::PushDefault => row.checked = r.push_default,
                    RemoteField::Prune => row.tri = tri_code(r.prune),
                }
            }
        }
        if let Kind::Button(a) = &opt.kind {
            if matches!(a, Action::RemoteRemove | Action::RemoteRename) {
                row.enabled &= self.remote.selected.is_some();
            }
            if matches!(a, Action::HookRemove | Action::HookCopy) {
                row.enabled &= self.hook_selected.is_some();
            }
        }
        row
    }

    // ----- edits -----

    fn edit_text(&mut self, i: usize, text: &str) {
        let Some(opt) = self.page().options.get(i).cloned() else {
            return;
        };
        match opt.store {
            Store::Reg(key, _) if opt.kind == Kind::Color => {
                // Only complete `#RRGGBB` values are stored.
                if let Some(c) = hex_to_colorref(text) {
                    self.settings.set(key, c);
                    self.reg_dirty = true;
                }
            }
            Store::Reg(key, _) => {
                self.settings.set(key, text);
                self.reg_dirty = true;
                self.note_network_change(key);
            }
            Store::Hook(HookField::Path) => self.hook_edit.path = text.to_owned(),
            Store::Hook(HookField::Command) => self.hook_edit.command = text.to_owned(),
            Store::Git(key) => self.set_git(key, Some(text.to_owned())),
            Store::ProgPath(key) => {
                let external = self.prog_is_external(key);
                self.settings.set(key, join_prog(external, text));
                self.reg_dirty = true;
            }
            Store::Remote(field) => {
                let r = &mut self.remote;
                match field {
                    RemoteField::Name => r.name = text.to_owned(),
                    RemoteField::Url => {
                        r.url = text.to_owned();
                        // Typing a URL with no remotes and no name pre-fills "origin".
                        if r.name.is_empty() && r.list.is_empty() {
                            r.name = "origin".to_owned();
                        }
                    }
                    RemoteField::PushUrl => r.push_url = text.to_owned(),
                    RemoteField::PuttyKey => r.putty_key = text.to_owned(),
                    _ => {}
                }
                r.dirty = true;
            }
            _ => {}
        }
    }

    fn edit_checked(&mut self, i: usize, checked: bool) {
        let Some(opt) = self.page().options.get(i).cloned() else {
            return;
        };
        match opt.store {
            Store::Reg(key, _) => {
                self.settings.set_bool(key, checked);
                self.reg_dirty = true;
                self.note_network_change(key);
            }
            Store::Hook(field) => {
                let h = &mut self.hook_edit;
                match field {
                    HookField::Enabled => h.enabled = checked,
                    HookField::Wait => h.wait = checked,
                    HookField::Hide => h.show = !checked,
                    HookField::ThisRepository => {
                        h.path = if checked {
                            self.cfg
                                .workdir()
                                .map(|w| w.to_string_lossy().into_owned())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                    }
                    _ => {}
                }
            }
            Store::BlockTrust(trust) => {
                let (mut block, mut t) =
                    block_trust(self.settings.dword("MergeBlockTrustBehavior", 0));
                if trust {
                    t = checked;
                } else {
                    block = checked;
                }
                self.settings
                    .set("MergeBlockTrustBehavior", block_trust_value(block, t));
                self.reg_dirty = true;
            }
            Store::Remote(RemoteField::PushDefault) => {
                self.remote.push_default = checked;
                self.remote.dirty = true;
            }
            _ => {}
        }
    }

    fn edit_tri(&mut self, i: usize, tri: i32) {
        let Some(opt) = self.page().options.get(i).cloned() else {
            return;
        };
        let value = match tri {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        };
        match opt.store {
            Store::Git(key) => self.set_git(key, value.map(|b| b.to_string())),
            Store::Remote(RemoteField::Prune) => {
                self.remote.prune = value;
                self.remote.dirty = true;
            }
            _ => {}
        }
    }

    fn edit_index(&mut self, i: usize, index: usize) {
        let Some(opt) = self.page().options.get(i).cloned() else {
            return;
        };
        let value = match &opt.kind {
            Kind::Combo {
                values: Some(v), ..
            }
            | Kind::Radio { values: v, .. } => v.get(index).cloned().unwrap_or_default(),
            _ => index.to_string(),
        };
        match opt.store {
            Store::Reg(key, _) => {
                self.settings.set(key, value);
                self.reg_dirty = true;
            }
            Store::Git(key) => self.set_git(key, (!value.is_empty()).then_some(value)),
            Store::ProgMode(key) => {
                self.prog_external.insert(key, index == 1);
                let (_, path) = split_prog(&self.reg(key, ""));
                self.settings.set(key, join_prog(index == 1, &path));
                self.reg_dirty = true;
            }
            Store::Remote(RemoteField::Tags) => {
                self.remote.tags = index;
                self.remote.dirty = true;
            }
            Store::Hook(HookField::Type) => {
                if let Some(t) = HookType::ALL.get(index) {
                    self.hook_edit.kind = *t;
                }
            }
            _ => {}
        }
    }

    fn note_network_change(&mut self, key: &str) {
        if key.contains("http-proxy-") {
            self.proxy_dirty = true;
        }
        if key == pages::SSH_CLIENT {
            self.ssh_dirty = true;
        }
    }

    // ----- hooks -----

    fn select_hook(&mut self, idx: Option<usize>) {
        self.hook_selected = idx.filter(|i| *i < self.hooks.len());
        self.hook_edit = self
            .hook_selected
            .map(|i| self.hooks[i].clone())
            .unwrap_or_else(new_hook);
    }

    fn store_hooks(&mut self) {
        self.settings
            .set(hooks::SETTINGS_KEY, hooks::serialize(&self.hooks));
        self.reg_dirty = true;
    }

    /// "Add New/Save": replaces the selected hook, or adds (replacing a hook
    /// with the same type and path, like `CHooks::Add`).
    fn save_hook(&mut self) -> Result<(), String> {
        let h = self.hook_edit.clone();
        if h.path.trim().is_empty() || h.command.trim().is_empty() {
            return Err("Both a path and a command line are required.".to_owned());
        }
        let idx = match self.hook_selected {
            Some(i) => {
                self.hooks[i] = h.clone();
                i
            }
            None => {
                self.hooks
                    .retain(|x| !(x.kind == h.kind && x.path == h.path));
                self.hooks.push(h.clone());
                self.hooks.len() - 1
            }
        };
        self.store_hooks();
        self.select_hook(Some(idx));
        Ok(())
    }

    fn remove_hook(&mut self) {
        if let Some(i) = self.hook_selected.take() {
            self.hooks.remove(i);
            self.store_hooks();
        }
        self.select_hook(None);
    }

    /// "Copy": the selected hook becomes a new, unsaved entry.
    fn copy_hook(&mut self) {
        self.hook_selected = None;
    }

    /// Writes `http.proxy` / `core.sshCommand` to the global git config
    /// (`SetProxyPage.cpp:224-270`).
    fn apply_network(&mut self) -> Result<(), String> {
        if self.proxy_dirty {
            let s = &self.settings;
            let url = koopagit_core::misccmds::proxy_url(
                s.bool(pages::PROXY_ENABLED, false),
                &s.string(pages::PROXY_HOST, ""),
                &s.string(pages::PROXY_PORT, ""),
                &s.string(pages::PROXY_USER, ""),
                &s.string(pages::PROXY_PASSWORD, ""),
            );
            self.cfg
                .store(ConfigScope::Global, "http.proxy", url.as_deref())
                .map_err(|e| format!("Saving http.proxy failed: {e}"))?;
            self.proxy_dirty = false;
        }
        if self.ssh_dirty {
            let ssh = self.settings.string(pages::SSH_CLIENT, "");
            let ssh = ssh.trim();
            self.cfg
                .store(
                    ConfigScope::Global,
                    "core.sshCommand",
                    (!ssh.is_empty()).then_some(ssh),
                )
                .map_err(|e| format!("Saving core.sshCommand failed: {e}"))?;
            self.ssh_dirty = false;
        }
        Ok(())
    }

    fn edit_inherit(&mut self, i: usize, inherit: bool) {
        let Some(Store::Git(key)) = self.page().options.get(i).map(|o| o.store.clone()) else {
            return;
        };
        let current = self
            .git_page()
            .and_then(|g| g.values.get(key).cloned().flatten());
        self.set_git(
            key,
            if inherit {
                None
            } else {
                Some(current.unwrap_or_default())
            },
        );
    }

    fn set_git(&mut self, key: &'static str, value: Option<String>) {
        if let Some(g) = self.git.get_mut(&self.current) {
            g.values.insert(key, value);
            g.dirty.insert(key);
        }
    }

    fn edit_checklist(&mut self, i: usize, checked: bool) {
        let Special::Checklist(ext) = self.page().special else {
            return;
        };
        let Some((bit, _)) = settings_entries().get(i).cloned() else {
            return;
        };
        let mask = self.menu_mask(ext);
        self.set_menu_mask(ext, if checked { mask | bit } else { mask & !bit });
    }

    fn checklist_select_all(&mut self) {
        let Special::Checklist(ext) = self.page().special else {
            return;
        };
        let all: u64 = settings_entries().iter().fold(0, |m, (b, _)| m | b);
        let mask = self.menu_mask(ext);
        self.set_menu_mask(
            ext,
            if mask & all == all {
                mask & !all
            } else {
                mask | all
            },
        );
    }

    fn checklist_restore(&mut self) {
        let Special::Checklist(ext) = self.page().special else {
            return;
        };
        self.set_menu_mask(
            ext,
            if ext {
                bits::DEFAULT_EXT
            } else {
                bits::DEFAULT_TOP
            },
        );
    }

    fn advanced(&self) -> Vec<KvRow> {
        self.settings
            .keys()
            .map(|k| KvRow {
                key: k.into(),
                value: self.settings.get(k).unwrap_or_default().into(),
            })
            .collect()
    }

    fn edit_advanced(&mut self, i: usize, value: &str) {
        let key = self.settings.keys().nth(i).map(str::to_owned);
        if let Some(k) = key {
            self.settings.set(&k, value);
            self.reg_dirty = true;
        }
    }

    // ----- remotes -----

    fn reload_remotes(&mut self, select: Option<&str>) {
        if !self.in_repo {
            return;
        }
        self.remote.list = self.cfg.remotes();
        let idx = select.and_then(|s| self.remote.list.iter().position(|r| r == s));
        self.select_remote(idx);
    }

    fn select_remote(&mut self, idx: Option<usize>) {
        let name = idx.and_then(|i| self.remote.list.get(i).cloned());
        let get = |k: String| self.cfg.get(ConfigScope::Local, &k, false);
        let r = match &name {
            Some(n) => RemoteEdit {
                list: std::mem::take(&mut self.remote.list),
                selected: idx,
                name: n.clone(),
                url: get(format!("remote.{n}.url")).unwrap_or_default(),
                push_url: get(format!("remote.{n}.pushurl")).unwrap_or_default(),
                putty_key: get(format!("remote.{n}.puttykeyfile")).unwrap_or_default(),
                tags: gitconfig::tagopt_index(get(format!("remote.{n}.tagopt")).as_deref()),
                push_default: get("remote.pushdefault".to_owned()).as_deref() == Some(n.as_str()),
                prune: gitconfig::tristate(get(format!("remote.{n}.prune")).as_deref()),
                dirty: false,
            },
            None => RemoteEdit {
                list: std::mem::take(&mut self.remote.list),
                ..Default::default()
            },
        };
        self.remote = r;
    }

    /// "Add New/Save" (`CSettingGitRemote::Save`). Returns an error message, or
    /// sets [`State::pending`] when a question must be answered first.
    fn save_remote(&mut self) -> Result<(), String> {
        let name = self.remote.name.trim().to_owned();
        let url = self.remote.url.trim().replace('\\', "/");
        if name.is_empty() {
            return Err("Remote name must not be empty.".to_owned());
        }
        if url.is_empty() {
            return Err("Remote URL must not be empty.".to_owned());
        }
        let exists = self.remote.list.contains(&name);
        let is_selected = self
            .remote
            .selected
            .and_then(|i| self.remote.list.get(i))
            .is_some_and(|s| *s == name);
        if exists && !is_selected && !self.overwrite_confirmed {
            self.pending = Pending::Overwrite;
            return Ok(());
        }
        let added = !exists;
        if added {
            if !self.remote.list.is_empty() && self.remote.tags != 1 {
                match self.tag_answer {
                    None => {
                        self.pending = Pending::TagOpt;
                        return Ok(());
                    }
                    Some(true) => self.remote.tags = 1,
                    Some(false) => {}
                }
            }
            self.cfg
                .run_git(&gitconfig::remote_add(&name, &url))
                .map_err(|e| e.to_string())?;
        }
        let r = self.remote.clone();
        let set = |key: String, value: Option<&str>| -> Result<(), String> {
            self.cfg
                .store(ConfigScope::Local, &key, value)
                .map_err(|_| {
                    format!(
                        "Saving config failed (key: \"{key}\", value: \"{}\").",
                        value.unwrap_or_default()
                    )
                })
        };
        set(format!("remote.{name}.url"), Some(&url))?;
        set(
            format!("remote.{name}.pushurl"),
            (!r.push_url.trim().is_empty()).then(|| r.push_url.trim()),
        )?;
        if cfg!(windows) {
            set(
                format!("remote.{name}.puttykeyfile"),
                (!r.putty_key.trim().is_empty()).then(|| r.putty_key.trim()),
            )?;
        }
        set(
            format!("remote.{name}.tagopt"),
            gitconfig::tagopt_value(r.tags),
        )?;
        let prune = r.prune.map(|b| b.to_string());
        set(format!("remote.{name}.prune"), prune.as_deref())?;
        let current_default = self
            .cfg
            .get(ConfigScope::Local, "remote.pushdefault", false);
        if r.push_default {
            set("remote.pushdefault".to_owned(), Some(&name))?;
        } else if current_default.as_deref() == Some(name.as_str()) {
            set("remote.pushdefault".to_owned(), None)?;
        }
        self.tag_answer = None;
        self.overwrite_confirmed = false;
        self.reload_remotes(Some(&name));
        if added && !self.no_fetch {
            self.pending = Pending::Fetch(name);
        }
        Ok(())
    }

    fn rename_remote(&mut self) -> Result<(), String> {
        let Some(old) = self
            .remote
            .selected
            .and_then(|i| self.remote.list.get(i).cloned())
        else {
            return Ok(());
        };
        let new = self.remote.name.trim().to_owned();
        if new.is_empty() {
            return Err("Remote name must not be empty.".to_owned());
        }
        if new != old {
            self.cfg
                .run_git(&gitconfig::remote_rename(&old, &new))
                .map_err(|e| e.to_string())?;
        }
        self.reload_remotes(Some(&new));
        Ok(())
    }

    fn remove_remote(&mut self, name: &str) -> Result<(), String> {
        self.cfg
            .run_git(&gitconfig::remote_remove(name))
            .map_err(|e| e.to_string())?;
        self.reload_remotes(None);
        Ok(())
    }

    // ----- apply -----

    fn dirty(&self) -> bool {
        self.reg_dirty || self.remote.dirty || self.git.values().any(|g| !g.dirty.is_empty())
    }

    /// Writes all pending changes. Sets [`Pending::SaveTo`] instead when a git
    /// page is saved to a different level than the one viewed.
    fn apply(&mut self) -> Result<(), String> {
        if !self.save_to_confirmed {
            let mismatch = self.git.values().any(|g| {
                !g.dirty.is_empty() && g.scope != ConfigScope::Effective && g.scope != g.save_to
            });
            if mismatch {
                self.pending = Pending::SaveTo;
                return Ok(());
            }
        }
        self.save_to_confirmed = false;
        if self.reg_dirty {
            self.settings.save().map_err(|e| e.to_string())?;
            self.reg_dirty = false;
        }
        self.apply_network()?;
        let pages: Vec<usize> = self.git.keys().copied().collect();
        for p in pages {
            let g = self.git[&p].clone();
            for key in &g.dirty {
                let v = g.values.get(key).cloned().flatten();
                self.cfg.store(g.save_to, key, v.as_deref()).map_err(|_| {
                    format!(
                        "Saving config failed (key: \"{key}\", value: \"{}\").",
                        v.unwrap_or_default()
                    )
                })?;
            }
            self.git.get_mut(&p).expect("page").dirty.clear();
        }
        if self.remote.dirty && self.remote.selected.is_some() {
            self.save_remote()?;
            self.remote.dirty = false;
        }
        Ok(())
    }

    fn save_to_target_label(&self) -> String {
        self.git
            .values()
            .find(|g| !g.dirty.is_empty())
            .and_then(|g| match g.save_to {
                ConfigScope::Local => self.cfg.file_of(ConfigScope::Local),
                ConfigScope::Project => self.cfg.project_file(),
                other => self.cfg.file_of(other),
            })
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn button(&mut self, action: Action) -> Result<(), String> {
        let open = |p: Option<PathBuf>| -> Result<(), String> {
            match p {
                Some(p) => editor::open(&p).map_err(|e| format!("{}: {e}", p.display())),
                None => Ok(()),
            }
        };
        match action {
            Action::EditLocalConfig => open(self.cfg.file_of(ConfigScope::Local)),
            Action::EditProjectConfig => open(self.cfg.project_file()),
            Action::EditGlobalConfig => open(self.cfg.file_of(ConfigScope::Global)),
            Action::EditXdgConfig => open(gitconfig::xdg_config_file()),
            Action::ViewSystemConfig => open(self.cfg.file_of(ConfigScope::System)),
            Action::ViewEffectiveConfig => {
                let text = self.cfg.effective_listing().map_err(|e| e.to_string())?;
                let path = temp_dir().join("effective-config.txt");
                std::fs::create_dir_all(temp_dir()).map_err(|e| e.to_string())?;
                std::fs::write(&path, text).map_err(|e| e.to_string())?;
                open(Some(path))
            }
            Action::RemoteSave => self.save_remote(),
            Action::RemoteRename => self.rename_remote(),
            Action::RemoteRemove => {
                if let Some(n) = self
                    .remote
                    .selected
                    .and_then(|i| self.remote.list.get(i).cloned())
                {
                    self.pending = Pending::Remove(n);
                }
                Ok(())
            }
            Action::ShowActionLog => open(Some(action_log())),
            Action::ClearActionLog => {
                let _ = std::fs::remove_file(action_log());
                Ok(())
            }
            Action::RestoreDefaults => {
                let keys: Vec<&'static str> = self
                    .page()
                    .options
                    .iter()
                    .filter_map(|o| match (&o.kind, &o.store) {
                        (Kind::Color, Store::Reg(k, _)) => Some(*k),
                        _ => None,
                    })
                    .collect();
                for k in keys {
                    self.settings.remove(k);
                }
                self.reg_dirty = true;
                Ok(())
            }
            Action::BrowseFolder(key) => {
                let start = PathBuf::from(self.reg(key, ""));
                if let Some(p) =
                    crate::pick::folder("Browse", start.is_dir().then_some(start.as_path()))
                {
                    self.settings.set(key, p.to_string_lossy());
                    self.reg_dirty = true;
                }
                Ok(())
            }
            Action::BrowseFile(key) => {
                if let Some(p) = crate::pick::open_file("Browse", None, &[]) {
                    let v = p.to_string_lossy().into_owned();
                    if self
                        .page()
                        .options
                        .iter()
                        .any(|o| o.store == Store::ProgPath(key))
                    {
                        self.prog_external.insert(key, true);
                        self.settings.set(key, join_prog(true, &v));
                    } else {
                        self.settings.set(key, v);
                    }
                    self.reg_dirty = true;
                    self.note_network_change(key);
                }
                Ok(())
            }
            Action::FirstStartWizard => {
                spawn_launcher(&Invocation::new("firststart"));
                Ok(())
            }
            Action::HookSave => self.save_hook(),
            Action::HookRemove => {
                self.remove_hook();
                Ok(())
            }
            Action::HookCopy => {
                self.copy_hook();
                Ok(())
            }
            Action::HookBrowsePath => {
                if let Some(p) = crate::pick::folder("Select folder to run script for", None) {
                    self.hook_edit.path = p.to_string_lossy().into_owned();
                }
                Ok(())
            }
            Action::HookBrowseCommand => {
                if let Some(p) = crate::pick::open_file("Select hook script file", None, &[]) {
                    self.hook_edit.command = p.to_string_lossy().into_owned();
                }
                Ok(())
            }
        }
    }
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join("koopagit")
}

/// KoopaGit does not write an action log yet; the path mirrors upstream's
/// `%LOCALAPPDATA%\TortoiseGit\logfile.txt` next to the settings file.
fn action_log() -> PathBuf {
    Settings::default_path()
        .and_then(|p| p.parent().map(|d| d.join("logfile.txt")))
        .unwrap_or_else(|| temp_dir().join("logfile.txt"))
}

fn kind_code(k: &Kind) -> i32 {
    match k {
        Kind::Group => 0,
        Kind::Note => 1,
        Kind::Check => 2,
        Kind::TriState => 3,
        Kind::Text => 4,
        Kind::Multiline => 5,
        Kind::Number => 6,
        Kind::Combo { .. } => 7,
        Kind::Radio { .. } => 8,
        Kind::Button(_) => 9,
        Kind::Clear(_) => 10,
        Kind::Color => 11,
    }
}

fn tri_code(v: Option<bool>) -> i32 {
    match v {
        Some(true) => 1,
        Some(false) => 0,
        None => 2,
    }
}

/// Fills a row's value fields from a stored string.
fn fill_value(row: &mut OptRow, kind: &Kind, value: Option<&str>, git: bool) {
    let v = value.unwrap_or_default();
    match kind {
        Kind::Check => {
            row.checked = if git {
                gitconfig::parse_bool(v) == Some(true)
            } else {
                v != "0" && !v.is_empty()
            }
        }
        Kind::TriState => row.tri = tri_code(value.and_then(gitconfig::parse_bool)),
        Kind::Text | Kind::Number | Kind::Multiline => row.text = v.into(),
        Kind::Combo { values: None, .. } => row.index = v.parse().unwrap_or(0),
        Kind::Combo {
            values: Some(values),
            ..
        }
        | Kind::Radio { values, .. } => {
            row.index = values.iter().position(|x| x == v).unwrap_or(0) as i32;
        }
        _ => {}
    }
}

/// Updates `model` in place so unchanged rows (and focused editors) survive.
fn sync_model<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    if model.row_count() != rows.len() {
        model.set_vec(rows);
        return;
    }
    for (i, r) in rows.into_iter().enumerate() {
        if model.row_data(i).as_ref() != Some(&r) {
            model.set_row_data(i, r);
        }
    }
}

/// Runs the dialog. `repo` is the repository the launcher was started in, if any.
pub fn run(repo: Option<&Repo>, settings: Settings, inv: &Invocation) -> DialogResult<i32> {
    let cfg = GitConfig::new(repo.map(Repo::workdir));
    let state = Rc::new(RefCell::new(State::new(
        repo,
        settings,
        cfg,
        inv.get("page"),
    )));
    let dlg = SettingsDialog::new()?;
    dlg.set_dialog_title(
        match repo {
            Some(r) => window_title(r.workdir(), "Settings"),
            None => "Settings - KoopaGit".to_owned(),
        }
        .into(),
    );
    let tree: Vec<TreeRow> = state
        .borrow()
        .pages
        .iter()
        .map(|p| TreeRow {
            label: p.label.into(),
            depth: i32::from(p.depth),
        })
        .collect();
    dlg.set_tree(ModelRc::new(VecModel::from(tree)));

    let rows = Rc::new(VecModel::<OptRow>::default());
    let checklist = Rc::new(VecModel::<CheckRow>::default());
    let advanced = Rc::new(VecModel::<KvRow>::default());
    let remotes = Rc::new(VecModel::<SharedString>::default());
    dlg.set_rows(ModelRc::from(rows.clone()));
    dlg.set_checklist(ModelRc::from(checklist.clone()));
    dlg.set_advanced(ModelRc::from(advanced.clone()));
    dlg.set_remotes(ModelRc::from(remotes.clone()));

    // Pushes the state into the UI. `full` re-creates page-level models.
    let refresh: Rc<dyn Fn(bool)> = {
        let state = state.clone();
        let weak = dlg.as_weak();
        let (rows, checklist, advanced, remotes) = (
            rows.clone(),
            checklist.clone(),
            advanced.clone(),
            remotes.clone(),
        );
        Rc::new(move |full: bool| {
            let Some(d) = weak.upgrade() else { return };
            let st = state.borrow();
            let page = st.page();
            if full {
                d.set_page(st.current as i32);
                d.set_page_title(page.label.into());
                d.set_stub(page.special == Special::Stub);
                rows.set_vec(st.rows());
            } else {
                sync_model(&rows, st.rows());
            }
            let ck = st.checklist();
            d.set_show_checklist(!ck.is_empty());
            if let Special::Checklist(ext) = page.special {
                d.set_checklist_caption(
                    if ext {
                        "Hide the selected items in the context menu:"
                    } else {
                        "Show the selected items on the first level of the context menu:"
                    }
                    .into(),
                );
                d.set_checklist_note(
                    if ext {
                        "Checked items will be hidden in the context menu by default and will only be visible if the shift key is pressed while opening the context menu."
                    } else {
                        "Unchecked items will appear in the KoopaGit submenu, checked items directly in the main context menu."
                    }
                    .into(),
                );
            }
            sync_model(&checklist, ck);
            d.set_show_advanced(page.special == Special::Advanced);
            if page.special == Special::Advanced {
                sync_model(&advanced, st.advanced());
            }
            let hooks_page = page.special == Special::Hooks;
            d.set_show_remotes(page.special == Special::Remote || hooks_page);
            let (names, selected): (Vec<SharedString>, Option<usize>) = if hooks_page {
                d.set_side_caption("Hook Scripts:".into());
                d.set_side_width(200.0);
                (
                    st.hooks.iter().map(|h| hook_label(h).into()).collect(),
                    st.hook_selected,
                )
            } else {
                d.set_side_caption("Remote:".into());
                d.set_side_width(160.0);
                (
                    st.remote.list.iter().map(|r| r.as_str().into()).collect(),
                    st.remote.selected,
                )
            };
            sync_model(&remotes, names);
            d.set_remote_index(selected.map_or(-1, |i| i as i32));
            match st.git_page() {
                Some(g) => {
                    let scopes = scope_list(g.honor_project);
                    let labels: Vec<SharedString> =
                        scopes.iter().map(|s| s.label().into()).collect();
                    let enabled: Vec<bool> = scopes
                        .iter()
                        .map(|s| {
                            st.in_repo || !matches!(s, ConfigScope::Local | ConfigScope::Project)
                        })
                        .collect();
                    d.set_show_config_source(true);
                    d.set_scope_labels(ModelRc::new(VecModel::from(labels)));
                    d.set_scope_enabled(ModelRc::new(VecModel::from(enabled)));
                    d.set_scope_index(scopes.iter().position(|s| *s == g.scope).unwrap_or(0) as i32);
                    let targets = st.save_to_list(g.honor_project);
                    let labels: Vec<SharedString> =
                        targets.iter().map(|s| s.label().into()).collect();
                    d.set_save_to_labels(ModelRc::new(VecModel::from(labels)));
                    d.set_save_to_index(
                        targets.iter().position(|s| *s == g.save_to).unwrap_or(0) as i32
                    );
                }
                None => d.set_show_config_source(false),
            }
            d.set_apply_enabled(st.dirty());
            let (text, buttons, error): (String, &[&str], bool) = match &st.pending {
                Pending::None => (String::new(), &[], false),
                Pending::SaveTo => (
                    format!("Are you sure you want to save to \"{}\"?", st.save_to_target_label()),
                    &["Save", "Abort"],
                    false,
                ),
                Pending::TagOpt => (
                    "To avoid fetching wrong tags, if this is not an official remote,\nyou are advised to disable tag fetching for this remote.\nDisable tag fetching?".to_owned(),
                    &["Yes", "No"],
                    false,
                ),
                Pending::Overwrite => (
                    format!("The remote \"{}\" already exists.\nDo you want to overwrite it?", st.remote.name.trim()),
                    &["Yes", "No"],
                    false,
                ),
                Pending::Fetch(_) => (
                    "Do you want to fetch remote branches from the newly added remote?".to_owned(),
                    &["Yes", "No"],
                    false,
                ),
                Pending::Remove(n) => (
                    format!("Do you really want to remove \"{n}\"?"),
                    &["Remove", "Abort"],
                    false,
                ),
            };
            // Errors are shown by `report` and kept until the next action.
            if !(text.is_empty() && d.get_notice_error()) {
                d.set_notice_text(text.into());
                let b: Vec<SharedString> = buttons.iter().map(|s| (*s).into()).collect();
                d.set_notice_buttons(ModelRc::new(VecModel::from(b)));
                d.set_notice_error(error);
            }
        })
    };
    let report: Rc<dyn Fn(Result<(), String>)> = {
        let weak = dlg.as_weak();
        Rc::new(move |r: Result<(), String>| {
            let Some(d) = weak.upgrade() else { return };
            d.set_notice_error(false);
            d.set_notice_text(SharedString::new());
            if let Err(e) = r {
                d.set_notice_text(e.into());
                d.set_notice_buttons(ModelRc::default());
                d.set_notice_error(true);
            }
        })
    };
    refresh(true);

    macro_rules! on {
        ($setter:ident, |$st:ident $(, $arg:ident : $ty:ty)*| $body:block, $full:expr) => {{
            let state = state.clone();
            let refresh = refresh.clone();
            let report = report.clone();
            dlg.$setter(move |$($arg: $ty),*| {
                let r: Result<(), String> = {
                    let $st = &mut *state.borrow_mut();
                    $body
                };
                report(r);
                refresh($full);
            });
        }};
    }

    on!(
        on_page_selected,
        |st, i: i32| {
            st.current = (i.max(0) as usize).min(st.pages.len() - 1);
            Ok(())
        },
        true
    );
    on!(
        on_opt_text,
        |st, i: i32, t: SharedString| {
            st.edit_text(i as usize, t.as_str());
            Ok(())
        },
        false
    );
    on!(
        on_opt_checked,
        |st, i: i32, c: bool| {
            st.edit_checked(i as usize, c);
            Ok(())
        },
        false
    );
    on!(
        on_opt_tri,
        |st, i: i32, s: i32| {
            st.edit_tri(i as usize, s);
            Ok(())
        },
        false
    );
    on!(
        on_opt_index,
        |st, i: i32, s: i32| {
            st.edit_index(i as usize, s.max(0) as usize);
            Ok(())
        },
        false
    );
    on!(
        on_opt_inherit,
        |st, i: i32, c: bool| {
            st.edit_inherit(i as usize, c);
            Ok(())
        },
        false
    );
    on!(
        on_opt_button,
        |st, i: i32| {
            let kind = st.page().options.get(i as usize).map(|o| o.kind.clone());
            match kind {
                Some(Kind::Button(a)) => st.button(a),
                Some(Kind::Clear(t)) => {
                    st.clear(t);
                    Ok(())
                }
                _ => Ok(()),
            }
        },
        false
    );
    on!(
        on_checklist_toggled,
        |st, i: i32, c: bool| {
            st.edit_checklist(i as usize, c);
            Ok(())
        },
        false
    );
    on!(
        on_checklist_select_all,
        |st| {
            st.checklist_select_all();
            Ok(())
        },
        false
    );
    on!(
        on_checklist_restore_default,
        |st| {
            st.checklist_restore();
            Ok(())
        },
        false
    );
    on!(
        on_scope_changed,
        |st, i: i32| {
            let page = st.current;
            if let Some(g) = st.git.get(&page) {
                let scope = scope_list(g.honor_project)[i.max(0) as usize];
                st.set_scope(page, scope);
            }
            Ok(())
        },
        true
    );
    on!(
        on_save_to_changed,
        |st, i: i32| {
            let page = st.current;
            if let Some(g) = st.git.get(&page) {
                let targets = st.save_to_list(g.honor_project);
                if let Some(t) = targets.get(i.max(0) as usize).copied() {
                    st.git.get_mut(&page).expect("page").save_to = t;
                }
            }
            Ok(())
        },
        false
    );
    on!(
        on_remote_selected,
        |st, i: i32| {
            if st.page().special == Special::Hooks {
                st.select_hook(Some(i.max(0) as usize));
            } else {
                st.select_remote(Some(i.max(0) as usize));
            }
            Ok(())
        },
        true
    );
    on!(
        on_advanced_edited,
        |st, i: i32, v: SharedString| {
            st.edit_advanced(i as usize, v.as_str());
            Ok(())
        },
        false
    );
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let report = report.clone();
        dlg.on_notice_clicked(move |i| {
            let pending = std::mem::replace(&mut state.borrow_mut().pending, Pending::None);
            let r = {
                let st = &mut *state.borrow_mut();
                match (pending, i) {
                    (Pending::SaveTo, 0) => {
                        st.save_to_confirmed = true;
                        st.apply()
                    }
                    (Pending::TagOpt, a) => {
                        st.tag_answer = Some(a == 0);
                        st.save_remote()
                    }
                    (Pending::Overwrite, 0) => {
                        st.overwrite_confirmed = true;
                        st.save_remote()
                    }
                    (Pending::Fetch(name), 0) => {
                        if let Some(w) = st.cfg.workdir() {
                            spawn_launcher(
                                &Invocation::new("fetch")
                                    .with_paths([w.to_owned()])
                                    .with("remote", name),
                            );
                        }
                        Ok(())
                    }
                    (Pending::Remove(name), 0) => st.remove_remote(&name),
                    _ => Ok(()),
                }
            };
            report(r);
            refresh(true);
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let report = report.clone();
        dlg.on_apply(move || {
            let r = state.borrow_mut().apply();
            report(r);
            refresh(false);
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let report = report.clone();
        let weak = dlg.as_weak();
        dlg.on_ok(move || {
            let r = state.borrow_mut().apply();
            let ok = r.is_ok() && state.borrow().pending == Pending::None;
            report(r);
            refresh(false);
            if ok {
                if let Some(d) = weak.upgrade() {
                    let _ = d.hide();
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

    crate::snapshot::arm(&dlg);
    dlg.run()?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use koopagit_core::git_cli::Git;

    fn state(dir: &tempfile::TempDir, in_repo: bool) -> State {
        let wt = dir.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        Git::new(&wt).run(["init", "-q"]).unwrap();
        let global = dir.path().join("global");
        std::fs::write(&global, "[user]\n\tname = G\n\temail = g@x\n").unwrap();
        let cfg = GitConfig::new(in_repo.then_some(wt.as_path()))
            .with_env("GIT_CONFIG_GLOBAL", &global)
            .with_env("GIT_CONFIG_NOSYSTEM", "1");
        let settings = Settings::load(&dir.path().join("settings.ini")).unwrap();
        let repo = in_repo.then(|| Repo::discover(&wt).unwrap());
        State::new(repo.as_ref(), settings, cfg, None)
    }

    fn goto(st: &mut State, id: &str) {
        st.current = st.pages.iter().position(|p| p.id == id).unwrap();
    }

    fn row_index(st: &State, label: &str) -> usize {
        st.page()
            .options
            .iter()
            .position(|o| o.label == label)
            .unwrap_or_else(|| panic!("{label}"))
    }

    #[test]
    fn registry_pages_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = state(&dir, false);
        assert_eq!(st.page().id, "main");
        goto(&mut st, "dialog2");
        let i = row_index(&st, "Disable sounds");
        assert!(!st.rows()[i].checked);
        st.edit_checked(i, true);
        let i = row_index(&st, "Autoclose Git.exe dialog:");
        st.edit_index(i, 2);
        goto(&mut st, "overlay");
        let radio = st
            .page()
            .options
            .iter()
            .position(|o| matches!(o.kind, Kind::Radio { .. }))
            .unwrap();
        assert_eq!(st.rows()[radio].index, 0);
        st.edit_index(radio, 3); // None -> CacheType 0
        let uam = row_index(&st, "Unversioned files mark parent folder as modified");
        assert!(!st.rows()[uam].enabled);
        goto(&mut st, "diff");
        st.edit_index(1, 1);
        st.edit_text(2, "meld %base %mine");
        assert!(st.dirty());
        st.apply().unwrap();
        let s = Settings::load(&dir.path().join("settings.ini")).unwrap();
        assert!(s.bool("NoSounds", false));
        assert_eq!(s.dword("AutoCloseGitProgress", 0), 2);
        assert_eq!(s.dword("CacheType", 1), 0);
        assert_eq!(s.string("Diff", ""), "meld %base %mine");
    }

    #[test]
    fn context_menu_masks() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = state(&dir, false);
        goto(&mut st, "look");
        let ck = st.checklist();
        let log = ck.iter().position(|c| c.label == "Show log").unwrap();
        let sync = ck.iter().position(|c| c.label == "Sync...").unwrap();
        assert!(ck[sync].checked && !ck[log].checked);
        st.edit_checklist(log, true);
        assert_eq!(st.menu_mask(false), bits::DEFAULT_TOP | bits::LOG);
        st.checklist_restore();
        assert_eq!(st.menu_mask(false), bits::DEFAULT_TOP);
        goto(&mut st, "extmenu");
        st.checklist_select_all();
        st.checklist_select_all();
        assert_eq!(st.menu_mask(true) & bits::STASH_APPLY, 0);
    }

    #[test]
    fn git_page_scopes_inherit_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = state(&dir, true);
        assert_eq!(st.page().id, "gitconfig");
        let g = &st.git[&st.current];
        assert_eq!(g.scope, ConfigScope::Effective);
        let name = row_index(&st, "Name:");
        let rows = st.rows();
        assert_eq!(rows[name].text.as_str(), "G");
        assert!(!rows[name].enabled, "Effective is read-only");
        let quote = row_index(&st, "QuotePath");
        assert_eq!(rows[quote].tri, 1, "unset quotepath shows git's default");

        let page = st.current;
        st.set_scope(page, ConfigScope::Local);
        assert!(st.rows()[name].inherited);
        st.edit_inherit(name, false);
        st.edit_text(name, "Local Me");
        let crlf = row_index(&st, "AutoCrLf:");
        st.edit_index(crlf, 3);
        st.apply().unwrap();
        assert_eq!(
            st.cfg
                .get(ConfigScope::Local, "user.name", false)
                .as_deref(),
            Some("Local Me")
        );
        assert_eq!(
            st.cfg
                .get(ConfigScope::Local, "core.autocrlf", false)
                .as_deref(),
            Some("input")
        );
        st.edit_inherit(name, true);
        st.apply().unwrap();
        assert_eq!(st.cfg.get(ConfigScope::Local, "user.name", false), None);

        // Saving to another level than the one viewed asks first.
        st.git.get_mut(&page).unwrap().save_to = ConfigScope::Global;
        st.edit_text(name, "x");
        st.apply().unwrap();
        assert_eq!(st.pending, Pending::SaveTo);
    }

    #[test]
    fn remote_page_add_rename_remove() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = state(&dir, true);
        goto(&mut st, "gitremote");
        let url = row_index(&st, "URL:");
        st.edit_text(url, r"C:\srv\repo.git");
        assert_eq!(st.remote.name, "origin");
        st.save_remote().unwrap();
        assert_eq!(st.pending, Pending::Fetch("origin".into()));
        st.pending = Pending::None;
        assert_eq!(st.remote.list, ["origin"]);
        assert_eq!(st.remote.url, "C:/srv/repo.git");

        // A second remote asks about tag fetching.
        st.select_remote(None);
        st.remote.name = "fork".into();
        st.remote.url = "/x".into();
        st.save_remote().unwrap();
        assert_eq!(st.pending, Pending::TagOpt);
        st.tag_answer = Some(true);
        st.save_remote().unwrap();
        assert_eq!(
            st.cfg
                .get(ConfigScope::Local, "remote.fork.tagopt", false)
                .as_deref(),
            Some("--no-tags")
        );

        st.remote.name = "mine".into();
        st.rename_remote().unwrap();
        assert_eq!(st.remote.list, ["mine", "origin"]);
        st.remove_remote("mine").unwrap();
        assert_eq!(st.remote.list, ["origin"]);
    }

    #[test]
    fn saved_data_counts_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = state(&dir, false);
        st.settings.push_history(r"History\PushURLS", "url", "a", 5);
        st.settings
            .push_history(r"History\commitX", "logmsgs", "m", 5);
        st.settings.set("PushAllBranches", 1);
        assert_eq!(st.clear_count(ClearTarget::UrlHistory), 1);
        assert_eq!(st.clear_count(ClearTarget::LogMessages), 1);
        assert_eq!(st.clear_count(ClearTarget::StoredDecisions), 1);
        st.clear(ClearTarget::UrlHistory);
        st.clear(ClearTarget::StoredDecisions);
        assert_eq!(st.clear_count(ClearTarget::UrlHistory), 0);
        assert_eq!(st.clear_count(ClearTarget::StoredDecisions), 0);
        assert_eq!(st.clear_count(ClearTarget::LogMessages), 1);
    }
}
