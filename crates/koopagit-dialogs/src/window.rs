//! Setup every KoopaGit window gets before it runs: the theme chosen under
//! Settings > General > Theme, and the `KOOPAGIT_SNAPSHOT` debug hook.

use std::sync::OnceLock;

use slint::ComponentHandle;

use crate::Theme;

/// Settings key of the Theme combo (a KoopaGit addition, docs/divergences.md).
pub const THEME_KEY: &str = "KoopaGitTheme";

/// 0 Classic, 1 Light, 2 Dark.
pub fn theme_style() -> i32 {
    static STYLE: OnceLock<i32> = OnceLock::new();
    *STYLE.get_or_init(|| {
        // Debug aid alongside KOOPAGIT_SNAPSHOT: render a theme without changing settings.
        if let Some(v) = std::env::var_os("KOOPAGIT_THEME") {
            return parse_theme(&v.to_string_lossy());
        }
        let settings = koopagit_core::settings::Settings::load_default();
        settings.dword(THEME_KEY, 0).min(2) as i32
    })
}

fn parse_theme(v: &str) -> i32 {
    match v.to_ascii_lowercase().as_str() {
        "1" | "light" => 1,
        "2" | "dark" => 2,
        _ => 0,
    }
}

/// Applies the theme to `component`; call before showing it.
pub fn apply_theme<C>(component: &C)
where
    C: ComponentHandle,
    for<'a> Theme<'a>: slint::Global<'a, C>,
{
    let style = theme_style();
    if style != 0 {
        component.global::<Theme>().set_style(style);
    }
}

/// Theme plus the snapshot hook: the common setup before `run()`/`show()`.
pub fn prepare<C>(component: &C)
where
    C: ComponentHandle + 'static,
    for<'a> Theme<'a>: slint::Global<'a, C>,
{
    apply_theme(component);
    crate::snapshot::arm(component);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names_and_numbers() {
        assert_eq!(parse_theme("dark"), 2);
        assert_eq!(parse_theme("Light"), 1);
        assert_eq!(parse_theme("2"), 2);
        assert_eq!(parse_theme("classic"), 0);
        assert_eq!(parse_theme("bogus"), 0);
    }
}
