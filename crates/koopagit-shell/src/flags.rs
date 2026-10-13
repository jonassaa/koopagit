//! `ITEMIS_*` selection flags (TortoiseShell `Globals.h`).

pub type ItemFlags = u32;

pub const ONLYONE: ItemFlags = 0x0000_0001;
pub const EXTENDED: ItemFlags = 0x0000_0002;
pub const INGIT: ItemFlags = 0x0000_0004;
pub const CONFLICTED: ItemFlags = 0x0000_0008;
pub const FOLDER: ItemFlags = 0x0000_0010;
pub const FOLDERINGIT: ItemFlags = 0x0000_0020;
pub const NORMAL: ItemFlags = 0x0000_0040;
pub const IGNORED: ItemFlags = 0x0000_0080;
pub const INVERSIONEDFOLDER: ItemFlags = 0x0000_0100;
pub const ADDED: ItemFlags = 0x0000_0200;
pub const DELETED: ItemFlags = 0x0000_0400;
pub const PATCHFILE: ItemFlags = 0x0000_1000;
pub const PATCHINCLIPBOARD: ItemFlags = 0x0000_8000;
pub const PATHINCLIPBOARD: ItemFlags = 0x0001_0000;
pub const TWO: ItemFlags = 0x0002_0000;
pub const SUBMODULECONTAINER: ItemFlags = 0x0004_0000;
pub const GITSVN: ItemFlags = 0x0008_0000;
pub const STASH: ItemFlags = 0x0010_0000;
pub const WCROOT: ItemFlags = 0x0020_0000;
pub const BISECT: ItemFlags = 0x0040_0000;
pub const BAREREPO: ItemFlags = 0x0080_0000;
pub const SUBMODULE: ItemFlags = 0x0100_0000;
pub const MERGEACTIVE: ItemFlags = 0x0200_0000;
pub const HASDIFFLATER: ItemFlags = 0x0400_0000;
pub const INACCESSIBLE: ItemFlags = 0x0800_0000;
