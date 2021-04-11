#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#[allow(dead_code)]
mod bindingsMbuf;

pub mod client;



// use std::ffi::CString;
// use std::os::raw::c_void;
// use std::ptr::null_mut;

// use std::process;
// use std::{thread, time};



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
