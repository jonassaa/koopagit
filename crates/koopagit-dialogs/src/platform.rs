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
}
