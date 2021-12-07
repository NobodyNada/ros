#[repr(u8)]
pub enum SyscallId {
    Exit,
    YieldCpu,
    Puts,
}
