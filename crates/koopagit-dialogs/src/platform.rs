//! Process-wide windowing setup, run by each GUI binary before its first window.

/// Selects the Slint backend.
///
/// On macOS the dialogs run from inside `KoopaGit.app`, whose Info.plist sets
/// `LSUIElement` so the Finder host stays out of the Dock. winit leaves a
/// bundled process's activation policy to that key, which would make every
/// dialog a UI element too: no Dock icon, no app switcher entry, and closing a
/// viewer hands focus to whichever app was active before (e.g. the terminal).
/// Ask for a regular app instead, like the TortoiseGit dialogs on the taskbar.
pub fn init() {
    #[cfg(target_os = "macos")]
    {
        use slint::winit_030::{
            SlintEvent,
            winit::event_loop::EventLoop,
            winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS},
        };
        let mut builder = EventLoop::<SlintEvent>::with_user_event();
        builder.with_activation_policy(ActivationPolicy::Regular);
        if let Err(e) = slint::BackendSelector::new()
            .with_winit_event_loop_builder(builder)
            .select()
        {
            eprintln!("koopagit: {e}");
        }
    }
    #[cfg(target_os = "linux")]
    linux::explain_missing_libraries();
}

/// winit loads the windowing libraries with `dlopen`, so a binary built fine
/// still fails on the first window when one is missing, as on minimal installs
/// such as WSL's Ubuntu. It panics about the keyboard libraries and errors
/// about the X11 ones; both are turned into the packages to install.
#[cfg(any(target_os = "linux", test))]
pub(crate) mod linux {
    /// What winit or xkbcommon-dl says when a library is missing, the library
    /// to name, and the packages that ship it on apt, dnf and pacman systems.
    const LIBRARIES: &[(&str, &str, [&str; 3])] = &[
        (
            "Library libxkbcommon-x11.so could not",
            "libxkbcommon-x11.so",
            ["libxkbcommon-x11-0", "libxkbcommon-x11", "libxkbcommon-x11"],
        ),
        (
            "Library libxkbcommon.so could not",
            "libxkbcommon.so",
            ["libxkbcommon0", "libxkbcommon", "libxkbcommon"],
        ),
        (
            "Failed to load one of xlib's shared libraries",
            "one of the X11 libraries",
            [
                "libx11-6 libx11-xcb1 libxcursor1 libxrandr2 libxi6",
                "libX11 libX11-xcb libXcursor libXrandr libXi",
                "libx11 libxcursor libxrandr libxi",
            ],
        ),
    ];

    #[cfg(target_os = "linux")]
    pub fn explain_missing_libraries() {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            match hint(info.payload_as_str().unwrap_or_default()) {
                Some(hint) => eprintln!("{hint}"),
                None => default_hook(info),
            }
        }));
    }

    /// What to install when `message` says a windowing library is missing.
    pub fn hint(message: &str) -> Option<String> {
        let (_, lib, [apt, dnf, pacman]) = LIBRARIES
            .iter()
            .find(|(needle, ..)| message.contains(needle))?;
        Some(format!(
            "koopagit: cannot open a window: {lib} is not installed.\n\
             Install it, or rerun install.sh, which installs every library the dialogs need:\n  \
             Debian/Ubuntu: sudo apt install {apt}\n  \
             Fedora:        sudo dnf install {dnf}\n  \
             Arch:          sudo pacman -S {pacman}"
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::hint;

        #[test]
        fn names_the_package_for_each_library() {
            let x11 = hint("Library libxkbcommon-x11.so could not be loaded.").unwrap();
            assert!(x11.contains("libxkbcommon-x11.so is not installed"));
            assert!(x11.contains("sudo apt install libxkbcommon-x11-0"));
            let base = hint("Library libxkbcommon.so could not be loaded.").unwrap();
            assert!(base.contains("sudo apt install libxkbcommon0"));
            let xlib = hint(
                "Error from Winit backend: Error initializing winit event loop: os error at \
                 linux/mod.rs:788: Failed to load one of xlib's shared libraries",
            )
            .unwrap();
            assert!(xlib.contains("one of the X11 libraries is not installed"));
            assert!(xlib.contains("sudo apt install libx11-6 libx11-xcb1"));
        }

        #[test]
        fn leaves_other_panics_alone() {
            assert_eq!(hint("index out of bounds"), None);
            assert_eq!(hint(""), None);
        }
    }
}
