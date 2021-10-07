use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

/// A wrapper allowing safe access to global hardware resources.
///
/// I/O devices must be used with caution due to thread-/interrupt-safety concerns.
/// This helper struct encapsulates a resource to ensure only one thread can access it at a time.
///
/// It also provides a lazy-initialization primitive so that drivers can guarantee hardware is
/// initialized at first access.
pub struct Global<T> {
    resource: UnsafeCell<Lazy<T>>,
    taken: AtomicBool,
}

enum Lazy<T> {
    Initializer(fn() -> T),
    Initialized(T),
}
impl<T> Lazy<T> {
    fn initialize(&mut self) {
        match self {
            Lazy::Initializer(f) => *self = Lazy::Initialized(f()),
            Lazy::Initialized(_) => {}
        }
    }

    fn get(&self) -> &T {
        match self {
            Lazy::Initializer(_) => panic!(),
            Lazy::Initialized(resource) => resource,
        }
    }

    fn get_mut(&mut self) -> &mut T {
        match self {
            Lazy::Initializer(_) => panic!(),
            Lazy::Initialized(resource) => resource,
        }
    }
}

impl<T> Global<T> {
    /// Initializes a global resource.
    pub const fn new(resource: T) -> Self {
        Global {
            resource: UnsafeCell::new(Lazy::Initialized(resource)),
            taken: AtomicBool::new(false),
        }
    }

    /// Creates a lazily-initialized global resource.
    pub const fn lazy(initializer: fn() -> T) -> Self {
        Global {
            resource: UnsafeCell::new(Lazy::Initializer(initializer)),
            taken: AtomicBool::new(false),
        }
    }

    /// Attempts to acquire exclusive access to this resource.
    pub fn take(&self) -> Option<GlobalGuard<'_, T>> {
        let was_taken = self.taken.swap(true, Ordering::Acquire);
        if !was_taken {
            unsafe {
                (*self.resource.get()).initialize();
            }
            Some(GlobalGuard(self))
        } else {
            None
        }
    }

    /// Makes a reference to the resource permanent. Future
    /// calls to `take` will always deny access.
    pub fn leak<'a>(guard: GlobalGuard<'a, T>) -> &'a mut T {
        let result = unsafe { (*guard.0.resource.get()).get_mut() };
        core::mem::forget(guard);
        result
    }

    /// Acquires a permanent reference to the resource.
    pub fn take_and_leak(&self) -> Option<&mut T> {
        self.take().map(|guard| Self::leak(guard))
    }
}
impl<T: Default> Default for Global<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}
impl<T: Default> Global<T> {
    pub const fn lazy_default() -> Self {
        Self::lazy(|| T::default())
    }
}
unsafe impl<T> Sync for Global<T> {}

/// A RAII guard protecting a global resource.
pub struct GlobalGuard<'a, T>(&'a Global<T>);
impl<'a, T> core::ops::Deref for GlobalGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { (*(self.0.resource.get() as *const Lazy<Self::Target>)).get() }
    }
}
impl<'a, T> core::ops::DerefMut for GlobalGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { (*self.0.resource.get()).get_mut() }
    }
}

impl<'a, T> core::ops::Drop for GlobalGuard<'a, T> {
    fn drop(&mut self) {
        self.0.taken.store(false, Ordering::Release);
    }
}
