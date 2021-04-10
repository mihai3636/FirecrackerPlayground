#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}



use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr::null_mut;

use std::process;
use std::{thread, time};

pub mod dpdk_component;

