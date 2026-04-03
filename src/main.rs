#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    if let Err(err) = myquickpaste_slint::app::run() {
        eprintln!("MyQuickPasteSlint failed to start: {err}");
        std::process::exit(1);
    }
}
