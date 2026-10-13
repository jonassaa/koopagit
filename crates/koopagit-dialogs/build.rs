fn main() {
    // Native-feeling widget style per OS (see docs/divergences.md).
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let style = if target_os == "macos" {
        "cupertino"
    } else {
        "fluent"
    };
    let config = slint_build::CompilerConfiguration::new().with_style(style.into());
    slint_build::compile_with_config("ui/dialogs.slint", config).expect("compile Slint UI");
}
