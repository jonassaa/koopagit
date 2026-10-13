//! Conflict-marker parsing and resolution for the three-way conflict editor.

/// One conflict block (`<<<<<<<` … `>>>>>>>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// "Mine" side (between `<<<<<<<` and `|||||||`/`=======`), lines with terminators.
    pub ours: Vec<String>,
    /// diff3 base section, if present.
    pub base: Option<Vec<String>>,
    /// "Theirs" side (between `=======` and `>>>>>>>`).
    pub theirs: Vec<String>,
    pub ours_label: String,
    pub theirs_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// Unconflicted lines (with terminators).
    Common(Vec<String>),
    Conflict(Conflict),
}

/// How a conflict block is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    #[default]
    Unresolved,
    Mine,
    Theirs,
    MineThenTheirs,
    TheirsThenMine,
}

fn marker<'a>(line: &'a str, m: &str) -> Option<&'a str> {
    let body = line.trim_end_matches(['\n', '\r']);
    let rest = body.strip_prefix(m)?;
    if rest.is_empty() {
        Some("")
    } else {
        rest.strip_prefix(' ')
    }
}

/// Parses git conflict markers. Unterminated blocks are kept as common text.
pub fn parse(text: &str) -> Vec<Segment> {
    #[derive(PartialEq)]
    enum St {
        Out,
        Ours,
        Base,
        Theirs,
    }
    let mut segs = Vec::new();
    let mut common: Vec<String> = Vec::new();
    let mut raw: Vec<String> = Vec::new();
    let mut cur = Conflict {
        ours: vec![],
        base: None,
        theirs: vec![],
        ours_label: String::new(),
        theirs_label: String::new(),
    };
    let mut st = St::Out;
    for line in text.split_inclusive('\n') {
        match st {
            St::Out => {
                if let Some(label) = marker(line, "<<<<<<<") {
                    st = St::Ours;
                    raw = vec![line.to_owned()];
                    cur = Conflict {
                        ours: vec![],
                        base: None,
                        theirs: vec![],
                        ours_label: label.to_owned(),
                        theirs_label: String::new(),
                    };
                } else {
                    common.push(line.to_owned());
                }
            }
            _ => {
                raw.push(line.to_owned());
                if st == St::Ours && marker(line, "|||||||").is_some() {
                    st = St::Base;
                    cur.base = Some(vec![]);
                } else if st != St::Theirs && marker(line, "=======").is_some_and(str::is_empty) {
                    st = St::Theirs;
                } else if st == St::Theirs
                    && let Some(label) = marker(line, ">>>>>>>")
                {
                    cur.theirs_label = label.to_owned();
                    if !common.is_empty() {
                        segs.push(Segment::Common(std::mem::take(&mut common)));
                    }
                    segs.push(Segment::Conflict(cur.clone()));
                    raw.clear();
                    st = St::Out;
                } else {
                    match st {
                        St::Ours => cur.ours.push(line.to_owned()),
                        St::Base => cur.base.get_or_insert_with(Vec::new).push(line.to_owned()),
                        St::Theirs => cur.theirs.push(line.to_owned()),
                        St::Out => {}
                    }
                }
            }
        }
    }
    common.extend(raw);
    if !common.is_empty() {
        segs.push(Segment::Common(common));
    }
    segs
}

pub fn has_conflicts(segments: &[Segment]) -> bool {
    segments.iter().any(|s| matches!(s, Segment::Conflict(_)))
}

/// Renders the merged text; `None` while any block is unresolved.
/// `choices` has one entry per conflict block, in order.
pub fn render(segments: &[Segment], choices: &[Choice]) -> Option<String> {
    let mut out = String::new();
    let mut k = 0;
    for s in segments {
        match s {
            Segment::Common(lines) => lines.iter().for_each(|l| out.push_str(l)),
            Segment::Conflict(c) => {
                let parts: Vec<&Vec<String>> = match choices.get(k).copied().unwrap_or_default() {
                    Choice::Unresolved => return None,
                    Choice::Mine => vec![&c.ours],
                    Choice::Theirs => vec![&c.theirs],
                    Choice::MineThenTheirs => vec![&c.ours, &c.theirs],
                    Choice::TheirsThenMine => vec![&c.theirs, &c.ours],
                };
                for p in parts {
                    p.iter().for_each(|l| out.push_str(l));
                    // Keep the next block on its own line if a side lacked a final newline.
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                k += 1;
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "a\n<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> feature\nb\n<<<<<<< ours\nm2\n||||||| base\nb2\n=======\nt2\n>>>>>>> theirs\n";

    #[test]
    fn parses_blocks_and_labels() {
        let segs = parse(TEXT);
        assert_eq!(segs.len(), 4);
        let Segment::Conflict(c) = &segs[1] else {
            panic!()
        };
        assert_eq!(c.ours, ["mine\n"]);
        assert_eq!(c.theirs, ["theirs\n"]);
        assert_eq!(
            (c.ours_label.as_str(), c.theirs_label.as_str()),
            ("HEAD", "feature")
        );
        let Segment::Conflict(c2) = &segs[3] else {
            panic!()
        };
        assert_eq!(c2.base.as_deref(), Some(&["b2\n".to_owned()][..]));
    }

    #[test]
    fn renders_choices() {
        let segs = parse(TEXT);
        assert!(render(&segs, &[Choice::Mine]).is_none());
        assert_eq!(
            render(&segs, &[Choice::Mine, Choice::TheirsThenMine]).unwrap(),
            "a\nmine\nb\nt2\nm2\n"
        );
        assert_eq!(
            render(&segs, &[Choice::Theirs, Choice::Theirs]).unwrap(),
            "a\ntheirs\nb\nt2\n"
        );
    }

    #[test]
    fn unterminated_block_is_text() {
        let segs = parse("x\n<<<<<<< a\ny\n");
        assert!(!has_conflicts(&segs));
        assert_eq!(render(&segs, &[]).unwrap(), "x\n<<<<<<< a\ny\n");
    }
}
