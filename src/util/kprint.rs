use core::fmt::{self, Write};

use crate::x86::io::{cga, serial};

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => (
        $crate::util::kprint::_kprint(core::format_args!($($arg)*))
    )
}

#[macro_export]
macro_rules! kprintln {
    ($($arg:tt)*) => (
        $crate::kprint!("{}\n", core::format_args!($($arg)*))
    )
}

#[doc(hidden)]
pub fn _kprint(fmt: fmt::Arguments<'_>) {
    serial::COM1
        .take()
        .expect("serial port conflict")
        .write_fmt(fmt)
        .expect("serial port error");

    // Also write to CGA, but ignore conflicts
    if let Some(mut cga) = cga::CGA.take() {
        cga.write_fmt(fmt).expect("CGA error");
    }
}
