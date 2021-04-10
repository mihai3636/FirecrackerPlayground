#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#[allow(dead_code)]
mod bindingsMbuf;

use bindingsMbuf::{
    rte_eal_init, rte_eal_process_type, rte_mbuf, rte_mempool_lookup,
    rte_proc_type_t_RTE_PROC_PRIMARY, rte_ring_dequeue_real, rte_ring_lookup,
};

pub fn play_winning_sound(name: String) {
    println!("Playing winning sound {}", name);
}
