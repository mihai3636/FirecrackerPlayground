#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#[allow(dead_code)]
mod bindingsMbuf;

mod dpdk_component;


// use std::ffi::CString;
// use std::os::raw::c_void;
// use std::ptr::null_mut;

// use std::process;
// use std::{thread, time};



// use bindingsMbuf::{
//     rte_eal_init, rte_eal_process_type, rte_mbuf, rte_mempool_lookup,
//     rte_proc_type_t_RTE_PROC_PRIMARY, rte_ring_dequeue_real, rte_ring_lookup,
// };

pub fn play_winning_sound(name: String) {
    println!("Playing winning sound {}", name);
    dpdk_component::test_func();
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
