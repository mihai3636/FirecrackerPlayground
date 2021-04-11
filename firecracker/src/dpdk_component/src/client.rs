use crate::bindingsMbuf::{
    rte_eal_init, rte_eal_process_type, rte_mbuf, rte_mempool_lookup,
    rte_proc_type_t_RTE_PROC_PRIMARY, rte_ring_dequeue_real, rte_ring_lookup,
};

use std::sync::mpsc::Receiver;
use logger::warn;


pub fn test_func() {
    println!("Hellooo, is this working?");
}

pub struct ClientDpdk {
    from_firecracker: Receiver<i32>,
}

impl ClientDpdk {
    pub fn new_with_receiver(receiver_channel: Receiver<i32>) -> ClientDpdk {
        warn!("Am creat un client DPDK nou! Yeey");
        ClientDpdk {
            from_firecracker: receiver_channel,
        }
    }
}