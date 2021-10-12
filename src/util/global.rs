#![allow(dead_code)]
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

/// A wrapper allowing safe access to global hardware resources.
///
/// I/O devices must be used with caution due to thread-/interrupt-safety concerns.
/// This helper struct encapsulates a resource to ensure only one thread can access it at a time.
///
/// It also provides a lazy-initialization primitive so that drivers can guarantee hardware is
/// initialized at first access.
pub struct Global<T> {
    resource: UnsafeCell<GlobalStorage<T>>,
    taken: AtomicBool,
}
impl<T> Global<T> {
    /// Initializes a global resource.
    pub const fn new(resource: T) -> Self {
        Global {
            resource: UnsafeCell::new(GlobalStorage::Initialized(resource)),
            taken: AtomicBool::new(false),
        }
    }

    /// Creates a lazily-initialized global resource.
    pub const fn lazy(initializer: fn() -> T) -> Self {
        Global {
            resource: UnsafeCell::new(GlobalStorage::Initializer(initializer)),
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
    pub fn leak(guard: GlobalGuard<'_, T>) -> &mut T {
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
        unsafe { (*(self.0.resource.get() as *const GlobalStorage<Self::Target>)).get() }
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

enum GlobalStorage<T> {
    Initializer(fn() -> T),
    Initialized(T),
}
impl<T> GlobalStorage<T> {
    fn initialize(&mut self) {
        match self {
            GlobalStorage::Initializer(f) => *self = GlobalStorage::Initialized(f()),
            GlobalStorage::Initialized(_) => {}
        }
    }

    fn get(&self) -> &T {
        match self {
            GlobalStorage::Initializer(_) => panic!(),
            GlobalStorage::Initialized(resource) => resource,
        }
    }

    unsafe fn get_mut(&mut self) -> &mut T {
        match self {
            GlobalStorage::Initializer(_) => panic!(),
            GlobalStorage::Initialized(resource) => resource,
        }
    }
}

/// A wrapper allowing a read-only resource to be safely initialized once.
pub struct Lazy<T> {
    resource: UnsafeCell<LazyStorage<T>>,
    state: AtomicU8,
}

impl<T> Lazy<T> {
    /// Creates a lazily-initialized resource.
    pub const fn new(initializer: fn() -> T) -> Self {
        Self {
            resource: UnsafeCell::new(LazyStorage { initializer }),
            state: AtomicU8::new(0),
        }
    }

    pub fn get(&self) -> &T {
        const LAZY_UNINIT: u8 = 0;
        const LAZY_INITING: u8 = 1;
        const LAZY_INIT: u8 = 2;

        match self.state.load(Ordering::Acquire) {
            // Fast path: if the resource is initialized, we're good to go.
            LAZY_INIT => unsafe { &(*self.resource.get()).resource },

            // If the resource is uninitialized, attempt to transform it into the "initializing" state.
            LAZY_UNINIT => match self.state.compare_exchange(
                LAZY_UNINIT,
                LAZY_INITING,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => unsafe {
                    // We successfully marked the resource as initializing, so initialize it.
                    let resource = &mut *self.resource.get();
                    resource.resource = core::mem::ManuallyDrop::new((resource.initializer)());
                    let resource = &*resource;

                    self.state.store(LAZY_INIT, Ordering::Release);
                    &resource.resource
                },
                Err(_) => {
                    // Extreme edge case: another thread wrote to the atomic variable in between us
                    // reading it and us updating it.
                    // That means something very thread-unsafe is going on.
                    panic!("Attempt to access lazy resource while it is being initialized")
                }
            },

            LAZY_INITING => {
                panic!("Attempt to access lazy resource while it is being initialized")
            }
            _ => unreachable!(),
        }
    }
}

union LazyStorage<T> {
    resource: core::mem::ManuallyDrop<T>,
    initializer: fn() -> T,
}
