//! Opens the Reset dialog on its own (it normally opens from the Log, RefLog
//! or Browse References dialogs). Usage: `reset_preview <workdir> [rev]`;
//! combine with `KOOPAGIT_SNAPSHOT=<png>` to render it.

fn main() -> Result<(), slint::PlatformError> {
    let mut args = std::env::args().skip(1);
    let workdir = args.next().unwrap_or_else(|| ".".to_owned());
    let rev = args.next().unwrap_or_else(|| "HEAD~1".to_owned());
    koopagit_dialogs::reset::show(workdir.into(), &rev, |ok| {
        println!("reset finished: {ok}");
    })?;
    slint::run_event_loop()
}
