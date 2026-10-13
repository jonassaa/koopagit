//! Overlay icons / badges (`CShellExt::IsMemberOf`, `IconOverlay.cpp`).

use std::path::Path;

use koopagit_core::Repo;
use koopagit_core::settings::{Settings, keys};
use koopagit_core::status::WcStatus;

use crate::pathfilter::PathFilter;

/// The nine TortoiseOverlays slots, in handler priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Overlay {
    Conflict,
    Modified,
    Deleted,
    /// assume-valid ("needs lock" in TortoiseSVN).
    ReadOnly,
    /// skip-worktree ("locked" in TortoiseSVN).
    Locked,
    Added,
    Normal,
    Ignored,
    Unversioned,
}

impl Overlay {
    pub const ALL: [Overlay; 9] = [
        Overlay::Conflict,
        Overlay::Modified,
        Overlay::Deleted,
        Overlay::ReadOnly,
        Overlay::Locked,
        Overlay::Added,
        Overlay::Normal,
        Overlay::Ignored,
        Overlay::Unversioned,
    ];

    /// Stable identifier used by platform extensions (badge / emblem names).
    pub fn id(self) -> &'static str {
        match self {
            Overlay::Conflict => "conflict",
            Overlay::Modified => "modified",
            Overlay::Deleted => "deleted",
            Overlay::ReadOnly => "readonly",
            Overlay::Locked => "locked",
            Overlay::Added => "added",
            Overlay::Normal => "normal",
            Overlay::Ignored => "ignored",
            Overlay::Unversioned => "unversioned",
        }
    }

    /// The `TortoiseOverlays` icon file stem (`<Name>Icon.ico`).
    pub fn icon_name(self) -> &'static str {
        match self {
            Overlay::Conflict => "Conflict",
            Overlay::Modified => "Modified",
            Overlay::Deleted => "Deleted",
            Overlay::ReadOnly => "ReadOnly",
            Overlay::Locked => "Locked",
            Overlay::Added => "Added",
            Overlay::Normal => "Normal",
            Overlay::Ignored => "Ignored",
            Overlay::Unversioned => "Unversioned",
        }
    }
}

/// Overlay-related settings (`ShellCache`).
#[derive(Debug, Clone)]
pub struct OverlaySettings {
    /// 0 none, 1 default (recursive), 2 shell, 3 shell extended.
    pub cache_type: u32,
    pub show_unversioned: bool,
    pub show_ignored: bool,
    pub show_excluded_as_normal: bool,
    pub unversioned_as_modified: bool,
    pub filter: PathFilter,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        OverlaySettings {
            cache_type: 1,
            show_unversioned: true,
            show_ignored: true,
            show_excluded_as_normal: true,
            unversioned_as_modified: false,
            filter: PathFilter::default(),
        }
    }
}

impl OverlaySettings {
    pub fn from_settings(s: &Settings) -> Self {
        OverlaySettings {
            cache_type: s.dword(keys::CACHE_TYPE, 1),
            show_unversioned: s.bool("ShowUnversionedOverlay", true),
            show_ignored: s.bool("ShowIgnoredOverlay", true),
            show_excluded_as_normal: s.bool(keys::SHOW_EXCLUDED_AS_NORMAL, true),
            unversioned_as_modified: s.bool(keys::UNVERSIONED_AS_MODIFIED, false),
            filter: PathFilter::new(
                &s.string(keys::OVERLAY_INCLUDE_LIST, ""),
                &s.string(keys::OVERLAY_EXCLUDE_LIST, ""),
            ),
        }
    }
}

/// Maps a status to the overlay shown (spec §2.2). Folders never show
/// Added/Deleted (`AdjustFolderStatus`): those become Modified.
pub fn overlay_for(
    status: WcStatus,
    is_dir: bool,
    assume_valid: bool,
    skip_worktree: bool,
    s: &OverlaySettings,
) -> Option<Overlay> {
    let status = match status {
        WcStatus::Added | WcStatus::Deleted if is_dir => WcStatus::Modified,
        other => other,
    };
    match status {
        WcStatus::None | WcStatus::Unknown => None,
        WcStatus::Unversioned => s.show_unversioned.then_some(Overlay::Unversioned),
        WcStatus::Ignored => s.show_ignored.then_some(Overlay::Ignored),
        WcStatus::Normal if skip_worktree => Some(Overlay::Locked),
        WcStatus::Normal if assume_valid => Some(Overlay::ReadOnly),
        WcStatus::Normal => Some(Overlay::Normal),
        WcStatus::Deleted => Some(Overlay::Deleted),
        WcStatus::Modified => Some(Overlay::Modified),
        WcStatus::Added => Some(Overlay::Added),
        WcStatus::Conflicted => Some(Overlay::Conflict),
    }
}

/// Full overlay lookup for one path, including the path filter and cache type.
pub fn overlay_for_path(path: &Path, s: &OverlaySettings) -> Option<Overlay> {
    if path.components().any(|c| c.as_os_str() == ".git") {
        return None;
    }
    let repo = Repo::discover(path).ok()?;
    let is_dir = path.is_dir();
    if !s.filter.is_allowed(path) {
        return (s.show_excluded_as_normal && is_dir).then_some(Overlay::Normal);
    }
    if s.cache_type == 0 {
        // "None": only versioned folders get an overlay.
        return is_dir.then_some(Overlay::Normal);
    }
    let status = repo
        .path_status_with(path, s.unversioned_as_modified)
        .ok()?;
    let (assume_valid, skip_worktree) = if is_dir {
        (false, false)
    } else {
        repo.index_flags(path)
    };
    overlay_for(status, is_dir, assume_valid, skip_worktree, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping() {
        let s = OverlaySettings::default();
        assert_eq!(
            overlay_for(WcStatus::Added, true, false, false, &s),
            Some(Overlay::Modified)
        );
        assert_eq!(
            overlay_for(WcStatus::Added, false, false, false, &s),
            Some(Overlay::Added)
        );
        assert_eq!(
            overlay_for(WcStatus::Normal, false, true, true, &s),
            Some(Overlay::Locked)
        );
        assert_eq!(
            overlay_for(WcStatus::Normal, false, true, false, &s),
            Some(Overlay::ReadOnly)
        );
        let hide = OverlaySettings {
            show_ignored: false,
            ..Default::default()
        };
        assert_eq!(
            overlay_for(WcStatus::Ignored, false, false, false, &hide),
            None
        );
    }
}
