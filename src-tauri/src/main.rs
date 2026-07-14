// Keep release builds from allocating a console window when launched directly.
// Debug builds retain the console so backend diagnostics remain visible.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lt_ai_coach_lib::run();
}
