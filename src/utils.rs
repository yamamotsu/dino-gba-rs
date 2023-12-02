use agb::mgba::{DebugLevel, Mgba};

pub fn print_info(mgba: &mut Option<Mgba>, output: core::fmt::Arguments) {
    // Debug output
    match mgba {
        Some(_mgba) => _mgba.print(output, DebugLevel::Info).unwrap(),
        None => {}
    };
}
