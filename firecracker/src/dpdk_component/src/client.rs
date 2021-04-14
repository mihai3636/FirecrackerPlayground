// use crate::bindingsMbuf::{
//     rte_eal_init, rte_eal_process_type, rte_mbuf, rte_mempool_lookup,
//     rte_proc_type_t_RTE_PROC_PRIMARY, rte_ring_dequeue_real, rte_ring_lookup,
// };

use crate::bindingsErrno::{
    rte_eal_init, rte_eal_process_type, rte_mbuf, rte_mempool_lookup,
    rte_proc_type_t_RTE_PROC_PRIMARY, rte_ring_dequeue_real, rte_ring_lookup,
};

use crate::Result;
use crate::Error;
use std::io;

use std::sync::mpsc::Receiver;
use std::{thread, time};

use std::ffi::{CString};
use std::os::raw::c_void;
use std::ptr::null_mut;

use logger::warn;


pub fn test_func() {
    println!("Hellooo, is this working?");
}

pub struct ClientDpdk {
    from_firecracker: Receiver<i32>,
}

impl ClientDpdk {
    pub fn new_with_receiver(receiver_channel: Receiver<i32>) -> ClientDpdk {
        warn!("New client has been created! Yeey!");
        ClientDpdk {
            from_firecracker: receiver_channel,
        }
    }
    /// Sets up the eal_init, first func to be called when using DPDK.
    fn do_rte_eal_init(&self) -> Result<()> {
        let m1 = CString::new("./executabil").expect("Nu a mers.\n");
        let m2 = CString::new("-l").expect("Nu a mers.\n");
        let m3 = CString::new("1").expect("Nu a mers.\n");
        let m4 = CString::new("--proc-type=secondary").expect("Nu a mers.\n");

        // You have to be careful to call as_ptr() separately.
        let mut args = vec![m1.as_ptr(), m2.as_ptr(), m3.as_ptr(), m4.as_ptr()];

        // no changes to args vector are allowed now! (The memory could be reallocated and you get dangling ptr)
        let my_args = args.as_mut_ptr();

        let cnt: i32 = 4;
        println!("Message before calling rte_eal_init!");
        let ret_val = unsafe { rte_eal_init(cnt, my_args) };
        println!("Message after calling rte_eal_init!");
        if 0 > ret_val {
            
            // ret_val is not the true errorcode
            // error code is inside rte_errno

            warn!("Eroare, nu a mers rte_eal_init.");
            warn!("Error code: {:?}", io::Error::last_os_error());
        
            return Err(Error::EalInitFailed(ret_val));
        } else {
            warn!("Este BAAAA A MERS NENOROCIREA MANCA-V-AS");
            return Ok(());
        }
    }

    pub fn start_dispatcher(&self) {
        self.do_rte_eal_init().expect("Failled rte_eal_init call");

        loop {
            match self.from_firecracker.recv_timeout(time::Duration::from_secs(20)) {
                Ok(numar) => { warn!("Received something! Number is: {}\n", numar) },
                Err(_) => { warn!("Nothing received.\n" )}
            };
        }
    }
}
