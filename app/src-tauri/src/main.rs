// NaCL is a graphical application, so debug and release builds both avoid
// opening a second console window on Windows.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    nacl_lib::run()
}
