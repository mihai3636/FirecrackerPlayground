
use crate::bindingsMbuf::{
    rte_eal_init, rte_eal_process_type, rte_mbuf, rte_mempool_lookup,
    rte_proc_type_t_RTE_PROC_PRIMARY, rte_ring_dequeue_real, rte_ring_lookup,
};

pub fn test_func() {
    println!("Hellooo, is this working?");
}