//! Windows Explorer shell extension: TortoiseGit's context menu
//! (`IShellExtInit` + `IContextMenu`) and status overlays
//! (`IShellIconOverlayIdentifier`, one COM class per overlay slot).
//!
//! Thin by design: menus and statuses come from `koopagit-shell`'s logic, and
//! every action starts `koopagit-proc.exe` (next to this DLL) as a separate
//! process, so a crash or hang never takes Explorer down.
//!
//! On non-Windows targets this crate is empty.
//!
//! Status: type-checked for `x86_64-pc-windows-msvc`; not yet loaded into a
//! real Explorer (see README.md).
#![cfg(windows)]

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use koopagit_core::settings::Settings;
use koopagit_shell::actions;
use koopagit_shell::menu::{Action, ContextMenu, Node, SUBMENU_LABEL};
use koopagit_shell::overlay::{Overlay, OverlaySettings, overlay_for_path};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, E_POINTER, HINSTANCE,
    S_FALSE, S_OK,
};
use windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IClassFactory, IClassFactory_Impl, IDataObject, TYMED_HGLOBAL,
};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Ole::{CF_HDROP, ReleaseStgMedium};
use windows::Win32::System::Registry::HKEY;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    CMF_DEFAULTONLY, CMF_EXTENDEDVERBS, CMINVOKECOMMANDINFO, DragQueryFileW, HDROP, IContextMenu,
    IContextMenu_Impl, ISIOI_ICONFILE, ISIOI_ICONINDEX, IShellExtInit, IShellExtInit_Impl,
    IShellIconOverlayIdentifier, IShellIconOverlayIdentifier_Impl, SHGetPathFromIDListW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, HMENU, InsertMenuW, MF_BYPOSITION, MF_POPUP, MF_SEPARATOR, MF_STRING,
};
use windows::core::{
    BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, PSTR, PWSTR, Ref, implement,
};

/// CLSID of the context-menu handler (`{5e1a8f00-...}` block is KoopaGit's own).
pub const CLSID_CONTEXT_MENU: GUID = GUID::from_u128(0x5e1a8f00_6b4c_4d1f_9a2e_7c0d3b1e4a00);

/// CLSIDs of the overlay handlers, in [`Overlay::ALL`] order.
pub const CLSID_OVERLAYS: [GUID; 9] = [
    GUID::from_u128(0x5e1a8f01_6b4c_4d1f_9a2e_7c0d3b1e4a01),
    GUID::from_u128(0x5e1a8f02_6b4c_4d1f_9a2e_7c0d3b1e4a02),
    GUID::from_u128(0x5e1a8f03_6b4c_4d1f_9a2e_7c0d3b1e4a03),
    GUID::from_u128(0x5e1a8f04_6b4c_4d1f_9a2e_7c0d3b1e4a04),
    GUID::from_u128(0x5e1a8f05_6b4c_4d1f_9a2e_7c0d3b1e4a05),
    GUID::from_u128(0x5e1a8f06_6b4c_4d1f_9a2e_7c0d3b1e4a06),
    GUID::from_u128(0x5e1a8f07_6b4c_4d1f_9a2e_7c0d3b1e4a07),
    GUID::from_u128(0x5e1a8f08_6b4c_4d1f_9a2e_7c0d3b1e4a08),
    GUID::from_u128(0x5e1a8f09_6b4c_4d1f_9a2e_7c0d3b1e4a09),
];

static DLL_INSTANCE: AtomicIsize = AtomicIsize::new(0);

/// The directory containing this DLL (where `koopagit-proc.exe` and the icons live).
fn module_dir() -> PathBuf {
    let mut buf = [0u16; 1024];
    let module = HINSTANCE(DLL_INSTANCE.load(Ordering::Relaxed) as *mut c_void);
    let len = unsafe { GetModuleFileNameW(Some(module.into()), &mut buf) } as usize;
    let path = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
    path.parent().map(Path::to_owned).unwrap_or_default()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---- overlays ---------------------------------------------------------------

/// One-entry cache shared by all overlay handlers: Explorer asks every handler
/// about the same path in turn (`g_filepath`/`g_filestatus` upstream).
static LAST_STATUS: Mutex<Option<(PathBuf, Option<Overlay>)>> = Mutex::new(None);

fn cached_overlay(path: &Path) -> Option<Overlay> {
    let mut last = LAST_STATUS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((p, o)) = last.as_ref()
        && p == path
    {
        return *o;
    }
    let settings = OverlaySettings::from_settings(&Settings::load_default());
    let overlay = overlay_for_path(path, &settings);
    *last = Some((path.to_owned(), overlay));
    overlay
}

#[implement(IShellIconOverlayIdentifier)]
struct OverlayHandler {
    overlay: Overlay,
}

impl IShellIconOverlayIdentifier_Impl for OverlayHandler_Impl {
    fn IsMemberOf(&self, pwszpath: &PCWSTR, _dwattrib: u32) -> windows::core::Result<()> {
        let path = PathBuf::from(unsafe { pwszpath.to_string() }.map_err(|_| E_INVALIDARG)?);
        if cached_overlay(&path) == Some(self.overlay) {
            Ok(())
        } else {
            Err(S_FALSE.into())
        }
    }

    fn GetOverlayInfo(
        &self,
        pwsziconfile: PWSTR,
        cchmax: i32,
        pindex: *mut i32,
        pdwflags: *mut u32,
    ) -> windows::core::Result<()> {
        if pwsziconfile.is_null() || pindex.is_null() || pdwflags.is_null() {
            return Err(E_POINTER.into());
        }
        let icon = module_dir()
            .join("icons")
            .join(format!("{}Icon.ico", self.overlay.icon_name()));
        let w = wide(&icon.to_string_lossy());
        if w.len() > cchmax as usize {
            return Err(E_FAIL.into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(w.as_ptr(), pwsziconfile.0, w.len());
            *pindex = 0;
            *pdwflags = ISIOI_ICONFILE | ISIOI_ICONINDEX;
        }
        Ok(())
    }

    /// Upstream priorities: Conflict 0 ... Normal 6; Ignored/Unversioned 100.
    fn GetPriority(&self) -> windows::core::Result<i32> {
        Ok(match self.overlay {
            Overlay::Conflict => 0,
            Overlay::Modified => 1,
            Overlay::Deleted => 2,
            Overlay::ReadOnly => 3,
            Overlay::Locked => 4,
            Overlay::Added => 5,
            Overlay::Normal => 6,
            Overlay::Ignored | Overlay::Unversioned => 100,
        })
    }
}

// ---- context menu -----------------------------------------------------------

#[derive(Default)]
struct MenuState {
    items: Vec<PathBuf>,
    background: Option<PathBuf>,
    /// Action per command offset (`idCmdFirst + index`).
    actions: Vec<Action>,
}

#[implement(IShellExtInit, IContextMenu)]
#[derive(Default)]
struct ContextMenuHandler {
    state: RefCell<MenuState>,
}

impl IShellExtInit_Impl for ContextMenuHandler_Impl {
    fn Initialize(
        &self,
        pidlfolder: *const ITEMIDLIST,
        pdtobj: Ref<IDataObject>,
        _hkeyprogid: HKEY,
    ) -> windows::core::Result<()> {
        let mut state = self.state.borrow_mut();
        *state = MenuState::default();
        if !pidlfolder.is_null() {
            let mut buf = [0u16; 260];
            if unsafe { SHGetPathFromIDListW(pidlfolder, &mut buf) }.as_bool() {
                let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                state.background = Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])));
            }
        }
        if let Some(data) = pdtobj.as_ref() {
            let format = FORMATETC {
                cfFormat: CF_HDROP.0,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            };
            let mut medium = unsafe { data.GetData(&format)? };
            let hdrop = HDROP(unsafe { medium.u.hGlobal.0 });
            let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
            for i in 0..count {
                let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
                let mut buf = vec![0u16; len + 1];
                unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) };
                state
                    .items
                    .push(PathBuf::from(String::from_utf16_lossy(&buf[..len])));
            }
            unsafe { ReleaseStgMedium(&mut medium) };
        }
        Ok(())
    }
}

impl ContextMenuHandler_Impl {
    /// Inserts `nodes` into `menu` at `pos`, recording actions; returns the next position.
    fn insert_nodes(
        &self,
        menu: HMENU,
        mut pos: u32,
        nodes: &[Node],
        first_id: u32,
        last_id: u32,
        actions: &mut Vec<Action>,
    ) -> u32 {
        for node in nodes {
            match node {
                Node::Separator => unsafe {
                    let _ = InsertMenuW(menu, pos, MF_BYPOSITION | MF_SEPARATOR, 0, PCWSTR::null());
                },
                Node::Item { label, action, .. } => {
                    let id = first_id + actions.len() as u32;
                    if id > last_id {
                        break;
                    }
                    actions.push(action.clone());
                    let w = wide(label);
                    unsafe {
                        let _ = InsertMenuW(
                            menu,
                            pos,
                            MF_BYPOSITION | MF_STRING,
                            id as usize,
                            PCWSTR(w.as_ptr()),
                        );
                    }
                }
                Node::Submenu { label, children } => {
                    let Ok(popup) = (unsafe { CreatePopupMenu() }) else {
                        continue;
                    };
                    self.insert_nodes(popup, 0, children, first_id, last_id, actions);
                    let w = wide(label);
                    unsafe {
                        let _ = InsertMenuW(
                            menu,
                            pos,
                            MF_BYPOSITION | MF_POPUP | MF_STRING,
                            popup.0 as usize,
                            PCWSTR(w.as_ptr()),
                        );
                    }
                }
            }
            pos += 1;
        }
        pos
    }
}

impl IContextMenu_Impl for ContextMenuHandler_Impl {
    fn QueryContextMenu(
        &self,
        hmenu: HMENU,
        indexmenu: u32,
        idcmdfirst: u32,
        idcmdlast: u32,
        uflags: u32,
    ) -> HRESULT {
        if uflags & CMF_DEFAULTONLY != 0 {
            return S_OK;
        }
        let settings = Settings::load_default();
        let (items, background) = {
            let s = self.state.borrow();
            (s.items.clone(), s.background.clone())
        };
        let extended = uflags & CMF_EXTENDEDVERBS != 0;
        let menu: ContextMenu =
            actions::query_menu(&items, background.as_deref(), extended, &settings);
        if menu.top.is_empty() && menu.submenu.is_empty() {
            return S_OK;
        }
        let mut acts = Vec::new();
        let mut nodes = vec![Node::Separator];
        nodes.extend(menu.top.iter().cloned());
        if !menu.submenu.is_empty() {
            nodes.push(Node::Submenu {
                label: SUBMENU_LABEL.to_owned(),
                children: menu.submenu.clone(),
            });
        }
        nodes.push(Node::Separator);
        self.insert_nodes(hmenu, indexmenu, &nodes, idcmdfirst, idcmdlast, &mut acts);
        let count = acts.len() as u32;
        self.state.borrow_mut().actions = acts;
        // MAKE_HRESULT(SEVERITY_SUCCESS, FACILITY_NULL, count)
        HRESULT(count as i32)
    }

    fn InvokeCommand(&self, pici: *const CMINVOKECOMMANDINFO) -> windows::core::Result<()> {
        let info = unsafe { pici.as_ref() }.ok_or(E_POINTER)?;
        let verb = info.lpVerb.0 as usize;
        if verb >> 16 != 0 {
            // String verbs are not supported.
            return Err(E_INVALIDARG.into());
        }
        let action = self
            .state
            .borrow()
            .actions
            .get(verb)
            .cloned()
            .ok_or(E_INVALIDARG)?;
        let dir = module_dir();
        let mut settings = Settings::load_default();
        actions::invoke_with(
            &actions::invoke_args(&action),
            &mut settings,
            &dir.join("koopagit-proc.exe"),
            &dir.join("koopagit-merge.exe"),
        )
        .map_err(|_| E_FAIL.into())
    }

    fn GetCommandString(
        &self,
        _idcmd: usize,
        _utype: u32,
        _preserved: *const u32,
        _pszname: PSTR,
        _cchmax: u32,
    ) -> windows::core::Result<()> {
        // Help strings are optional; Explorer copes with E_NOTIMPL-like failures.
        Err(E_INVALIDARG.into())
    }
}

// ---- class factory and DLL exports ------------------------------------------

#[derive(Clone, Copy)]
enum Kind {
    ContextMenu,
    Overlay(Overlay),
}

#[implement(IClassFactory)]
struct Factory {
    kind: Kind,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if punkouter.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        if riid.is_null() || ppvobject.is_null() {
            return Err(E_POINTER.into());
        }
        let unknown: IUnknown = match self.kind {
            Kind::ContextMenu => ContextMenuHandler::default().into(),
            Kind::Overlay(overlay) => OverlayHandler { overlay }.into(),
        };
        unsafe { unknown.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _flock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

/// # Safety
/// Called by COM with valid out-pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if rclsid.is_null() || ppv.is_null() {
        return E_POINTER;
    }
    let clsid = unsafe { *rclsid };
    let kind = if clsid == CLSID_CONTEXT_MENU {
        Kind::ContextMenu
    } else if let Some(i) = CLSID_OVERLAYS.iter().position(|c| *c == clsid) {
        Kind::Overlay(Overlay::ALL[i])
    } else {
        return CLASS_E_CLASSNOTAVAILABLE;
    };
    let factory: IClassFactory = Factory { kind }.into();
    unsafe { factory.query(riid, ppv) }
}

/// Explorer may unload us when nothing is in use; staying loaded is simpler and safe.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

/// # Safety
/// Called by the loader.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(instance: HINSTANCE, reason: u32, _: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        DLL_INSTANCE.store(instance.0 as isize, Ordering::Relaxed);
    }
    BOOL(1)
}
