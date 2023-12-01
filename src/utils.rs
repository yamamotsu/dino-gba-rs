use agb::mgba::{DebugLevel, Mgba};

pub fn print_info(mgba: &mut Mgba, output: core::fmt::Arguments) {
    // Debug output
    mgba.print(output, DebugLevel::Info).unwrap();
}
