//! Renders the Log dialog with a multi-row selection and the Statistics
//! window, for reviewing layouts with `KOOPAGIT_SNAPSHOT` (no repository needed).
//!
//! `cargo run --example log_preview -- log` or `-- stats`.

use koopagit_dialogs::{GraphPath, LogDialog, LogRow, RefChip, StatRow, StatisticsDialog};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

/// Lane lines: (path commands, colour), lane count, node lane, merge.
type Graph = (&'static [(&'static str, i32)], i32, i32, bool);

fn row(message: &str, selected: bool, wc: bool, chip: Option<(&str, i32)>, g: Graph) -> LogRow {
    LogRow {
        graph: ModelRc::new(VecModel::from(
            g.0.iter()
                .map(|&(c, color)| GraphPath {
                    commands: c.into(),
                    color,
                })
                .collect::<Vec<_>>(),
        )),
        lanes: g.1,
        node: g.2,
        merge: g.3,
        refs: ModelRc::new(VecModel::from(
            chip.map(|(n, k)| RefChip {
                name: n.into(),
                kind: k,
            })
            .into_iter()
            .collect::<Vec<_>>(),
        )),
        message: message.into(),
        author: if wc { "" } else { "Ann" }.into(),
        date: if wc { "" } else { "2026-10-13 12:00" }.into(),
        sha: "0123abcd".into(),
        is_head: message == "Second",
        selected,
        wc,
    }
}

fn stat(l: &str, v: &str) -> StatRow {
    StatRow {
        label: l.into(),
        value: v.into(),
    }
}

fn main() -> Result<(), slint::PlatformError> {
    if std::env::args().nth(1).as_deref() == Some("stats") {
        let d = StatisticsDialog::new()?;
        d.set_summary(ModelRc::new(VecModel::from(vec![
            stat("Number of weeks", "2"),
            stat("Number of authors", "2"),
            stat("Total commits", "5"),
            stat("Most active author", "Ann"),
        ])));
        d.set_per_author(ModelRc::new(VecModel::from(vec![
            stat("Ann", "3"),
            stat("Bob", "2"),
        ])));
        d.set_per_week(ModelRc::new(VecModel::from(vec![
            stat("2026-10-05", "2"),
            stat("2026-10-12", "3"),
        ])));
        koopagit_dialogs::snapshot::arm(&d);
        return d.run();
    }
    let d = LogDialog::new()?;
    d.set_rows(ModelRc::new(VecModel::from(vec![
        row(
            "Working tree changes",
            false,
            true,
            None,
            (&[("M 8 11 L 8 22", 0)], 1, 0, false),
        ),
        row(
            "Second",
            true,
            false,
            Some(("main", 0)),
            (&[("M 8 0 L 8 22", 0)], 1, 0, false),
        ),
        row(
            "Merge feature",
            true,
            false,
            Some(("v1.0", 3)),
            (
                &[
                    ("M 8 11 L 8 0 M 8 11 L 8 22 ", 0),
                    ("M 8 11 L 16 11 C 20.42 11 24 17 24 22 ", 1),
                ],
                2,
                0,
                true,
            ),
        ),
        row(
            "feature work",
            true,
            false,
            Some(("feature", 1)),
            (
                &[
                    ("M 8 11 L 8 0 M 8 11 L 8 22 ", 0),
                    ("M 24 11 L 24 0 M 24 11 L 24 22 ", 1),
                ],
                2,
                1,
                false,
            ),
        ),
        row(
            "First",
            false,
            false,
            None,
            (
                &[
                    ("M 8 11 L 8 0 ", 0),
                    ("M 8 11 L 16 11 C 20.42 11 24 4.92 24 0 ", 1),
                ],
                2,
                0,
                false,
            ),
        ),
    ])));
    d.set_current_row(3);
    d.set_ref_label("<All Branches>".into());
    d.set_all_branches(true);
    d.set_date_from("2026-01-01".into());
    d.set_date_to("2026-12-31".into());
    d.set_jump_types(ModelRc::new(VecModel::from(vec![SharedString::from(
        "Merge Point",
    )])));
    d.set_details("SHA-1: 0123abcd\n\nfeature work\n".into());
    d.set_info_text(
        "Showing 4 revision(s), from revision 0123abcd to revision 0123abcd - 3 revision(s) selected, 0 file(s) selected"
            .into(),
    );
    koopagit_dialogs::snapshot::arm(&d);
    d.run()
}
