#![allow(dead_code, clippy::missing_safety_doc)]

pub mod serial;

/// An x86 I/O port (accessed using the 'in' and 'out' instructions).
///
/// An I/O port is addressed using a base address and an optional offset (because Rust does not yet
/// support arithmetic in a const generic context.)
///
/// # Safety
///
/// All I/O functions are unsafe, because they do not guarantee freedom from race conditions. It is
/// the caller's responsibility to ensure I/O ports are accessed in a thread-safe manner.
/// 
/// Prefer using safe wrappers (such as `Serial`) to perform I/O.
#[derive(Clone)]
#[rustfmt::skip] // https://github.com/rust-lang/rustfmt/issues/4816
pub struct Io<T: IoRw, const BASE: u16, const OFFSET: u16 = 0>(core::marker::PhantomData<T>);

impl<T: IoRw, const BASE: u16, const OFFSET: u16> Io<T, BASE, OFFSET> {
    /// Instantiates an I/O port.
    pub const fn new() -> Self {
        Self(core::marker::PhantomData {})
    }

    /// Reads a single value from the port.
    pub unsafe fn read(&mut self) -> T {
        T::read(BASE + OFFSET)
    }
    /// Writes a single value to the port.
    pub unsafe fn write(&mut self, val: T) {
        val.write(BASE + OFFSET)
    }
}
impl<T: IoRw + IoRwSlice, const BASE: u16, const OFFSET: u16> Io<T, BASE, OFFSET> {
    /// Reads several values from the port.
    pub unsafe fn read_slice(&mut self, slice: &mut [T]) {
        T::read_slice(slice, BASE + OFFSET)
    }

    /// Writes several values to the port.
    pub unsafe fn write_slice(&mut self, slice: &[T]) {
        T::write_slice(slice, BASE + OFFSET)
    }
}

impl<T: IoRw, const BASE: u16, const OFFSET: u16> Default for Io<T, BASE, OFFSET> {
    fn default() -> Self {
        Self::new()
    }
}

/// An input-only port. See `Io` for more details.
#[derive(Clone)]
pub struct Input<T: IoRw, const BASE: u16, const OFFSET: u16>(Io<T, BASE, OFFSET>);
impl<T: IoRw, const BASE: u16, const OFFSET: u16> Input<T, BASE, OFFSET> {
    /// Instantiates an input port.
    pub const fn new() -> Self {
        Self(Io::new())
    }

    /// Reads a value from the input port.
    pub unsafe fn read(&mut self) -> T {
        self.0.read()
    }
}
impl<T: IoRw + IoRwSlice, const BASE: u16, const OFFSET: u16> Input<T, BASE, OFFSET> {
    /// Reads several values from the input port.
    pub unsafe fn read_slice(&mut self, slice: &mut [T]) {
        self.0.read_slice(slice)
    }
}
impl<T: IoRw, const BASE: u16, const OFFSET: u16> Default for Input<T, BASE, OFFSET> {
    fn default() -> Self {
        Self::new()
    }
}

/// An output-only port. See `Io` for more details.
#[derive(Clone)]
pub struct Output<T: IoRw, const BASE: u16, const OFFSET: u16>(Io<T, BASE, OFFSET>);
impl<T: IoRw, const BASE: u16, const OFFSET: u16> Output<T, BASE, OFFSET> {
    /// Instantiates a new output port.
    pub const fn new() -> Self {
        Self(Io::new())
    }

    /// Writes a single value to the output pot.
    pub unsafe fn write(&mut self, val: T) {
        self.0.write(val)
    }
}
impl<T: IoRw + IoRwSlice, const BASE: u16, const OFFSET: u16> Output<T, BASE, OFFSET> {
    /// Writes several values to the output port.
    pub unsafe fn write_slice(&mut self, slice: &[T]) {
        self.0.write_slice(slice)
    }
}
impl<T: IoRw, const BASE: u16, const OFFSET: u16> Default for Output<T, BASE, OFFSET> {
    fn default() -> Self {
        Self::new()
    }
}

/// A type that can be transferred over an x86 I/O port.
pub trait IoRw {
    /// Reads a single value from the port.
    unsafe fn read(port: u16) -> Self;
    /// Writes a single value to the port.
    unsafe fn write(&self, port: u16);
}

/// A type that can be transferred in bulk over an x86 I/O port
/// using the 'rep ins/outs' family oo instructions.
pub trait IoRwSlice: Sized {
    /// Reads several values from the port.
    unsafe fn read_slice(slice: &mut [Self], port: u16);
    /// Writes several values to the port.
    unsafe fn write_slice(slice: &[Self], port: u16);
}

/// A convenience trait for defining types (like register bitfields)
/// that can be transferred over an I/O port.
///
/// # Example
///
/// ```
/// struct SomeRegister(u8);
/// impl From<u8> for SomeRegister {
///     fn from(x: u8) -> Self { Self(x) }
/// }
/// impl From<SomeRegister> for u8 {
///     fn from(x: SomeRegister) -> Self { x.0 }
/// }
///
/// impl IoRwConvertible for SomeRegister {
///     type Io = u8;
/// }
/// ```
pub trait IoRwConvertible: Copy {
    type Io: IoRw + From<Self> + Into<Self>;
}
impl<T: IoRwConvertible> IoRw for T {
    unsafe fn read(port: u16) -> Self {
        T::Io::read(port).into()
    }
    unsafe fn write(&self, port: u16) {
        T::Io::from(*self).write(port)
    }
}

impl IoRw for u8 {
    unsafe fn read(port: u16) -> Self {
        let result: u8;
        asm!("in al, dx", out("al") result, in("dx") port, options(nomem, nostack));
        result
    }
    unsafe fn write(&self, port: u16) {
        asm!("out dx, al", in("dx") port, in("al") *self, options(nomem, nostack));
    }
}
impl IoRwSlice for u8 {
    unsafe fn read_slice(slice: &mut [Self], port: u16) {
        asm!("rep insb [edi], dx", in("edi") slice.as_ptr(), in("ecx") slice.len(), in("dx") port, options(nostack));
    }
    unsafe fn write_slice(slice: &[Self], port: u16) {
        asm!("rep outsb [edi], dx", in("edi") slice.as_ptr(), in("ecx") slice.len(), in("dx") port, options(nostack));
    }
}

impl IoRw for u16 {
    unsafe fn read(port: u16) -> Self {
        let result: u16;
        asm!("in ax, dx", out("ax") result, in("dx") port, options(nomem, nostack));
        result
    }
    unsafe fn write(&self, port: u16) {
        asm!("out dx, ax", in("dx") port, in("ax") *self, options(nomem, nostack));
    }
}
impl IoRwSlice for u16 {
    unsafe fn read_slice(slice: &mut [Self], port: u16) {
        asm!("rep insw [edi], dx", in("edi") slice.as_ptr(), in("ecx") slice.len(), in("dx") port, options(nostack));
    }
    unsafe fn write_slice(slice: &[Self], port: u16) {
        asm!("rep outsw [edi], dx", in("edi") slice.as_ptr(), in("ecx") slice.len(), in("dx") port, options(nostack));
    }
}

impl IoRw for u32 {
    unsafe fn read(port: u16) -> Self {
        let result: u32;
        asm!("in eax, dx", out("eax") result, in("dx") port, options(nomem, nostack));
        result
    }
    unsafe fn write(&self, port: u16) {
        asm!("out dx, eax", in("dx") port, in("eax") *self, options(nomem, nostack));
    }
}
impl IoRwSlice for u32 {
    unsafe fn read_slice(slice: &mut [Self], port: u16) {
        asm!("rep insd [edi], dx", in("edi") slice.as_ptr(), in("ecx") slice.len(), in("dx") port, options(nostack));
    }
    unsafe fn write_slice(slice: &[Self], port: u16) {
        asm!("rep outsd [edi], dx", in("edi") slice.as_ptr(), in("ecx") slice.len(), in("dx") port, options(nostack));
    }
}

impl IoRw for i8 {
    unsafe fn read(port: u16) -> Self {
        u16::read(port) as Self
    }
    unsafe fn write(&self, port: u16) {
        (*self as u16).write(port)
    }
}
impl IoRwSlice for i8 {
    unsafe fn read_slice(slice: &mut [Self], port: u16) {
        u8::read_slice(
            core::slice::from_raw_parts_mut(slice.as_mut_ptr() as _, slice.len()),
            port,
        )
    }
    unsafe fn write_slice(slice: &[Self], port: u16) {
        u8::write_slice(
            core::slice::from_raw_parts(slice.as_ptr() as _, slice.len()),
            port,
        )
    }
}

impl IoRw for i16 {
    unsafe fn read(port: u16) -> Self {
        u16::read(port) as Self
    }
    unsafe fn write(&self, port: u16) {
        (*self as u16).write(port)
    }
}
impl IoRwSlice for i16 {
    unsafe fn read_slice(slice: &mut [Self], port: u16) {
        u16::read_slice(
            core::slice::from_raw_parts_mut(slice.as_mut_ptr() as _, slice.len()),
            port,
        )
    }
    unsafe fn write_slice(slice: &[Self], port: u16) {
        u16::write_slice(
            core::slice::from_raw_parts(slice.as_ptr() as _, slice.len()),
            port,
        )
    }
}

impl IoRw for i32 {
    unsafe fn read(port: u16) -> Self {
        u16::read(port) as Self
    }
    unsafe fn write(&self, port: u16) {
        (*self as u32).write(port)
    }
}
impl IoRwSlice for i32 {
    unsafe fn read_slice(slice: &mut [Self], port: u16) {
        u32::read_slice(
            core::slice::from_raw_parts_mut(slice.as_mut_ptr() as _, slice.len()),
            port,
        )
    }
    unsafe fn write_slice(slice: &[Self], port: u16) {
        u32::write_slice(
            core::slice::from_raw_parts(slice.as_ptr() as _, slice.len()),
            port,
        )
    }
}
