#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#[allow(dead_code)]
mod bindingsMbuf;

pub mod client;

use std::result;
// got this from net virtio::net to avoid package dependency
const MAX_BUFFER_SIZE: usize = 65562;

#[derive(Debug)]
pub enum Error {
    /// Failed to do rte_eal_init()
    EalInitFailed(i32),
    NotSecondaryDpdk,
    RingLookupFailed,
    MempoolLookupFailed,
    MempoolGetFailed,
    RingEnqueueFailed,
    RingDequeueFailed,
}

pub type Result<T> = result::Result<T, Error>;
pub type ArrayTuple = std::boxed::Box<([u8; MAX_BUFFER_SIZE], usize)>;
 

pub fn play_winning_sound(name: String) {
    println!("Playing winning sound {}", name);
    client::test_func();
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
