//! ROS userland runtime library
//!
//! Defines data structures and functions ROS programs can use to communicate with the kernel.

#![no_std]
#![feature(lang_items)]

// export syscall_common and syscall_user as syscall
mod syscall_common;
mod syscall_user;
pub mod syscall {
    //! The syscall module is defined in two files: syscall_common.rs and syscall_user.rs. Common
    //! contains data structures and definitions used by both userspace and kernelspace, whereas
    //! User contains only those used by uernelspace.
    pub use super::{syscall_common::*, syscall_user::*};
}

// Re-export the 'roslib' directory
mod roslib;
pub use roslib::*;
