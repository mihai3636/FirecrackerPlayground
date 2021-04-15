use crate::bindingsMbuf::{
    rte_eal_init, rte_eal_process_type, rte_mbuf, rte_mempool_lookup,
    rte_proc_type_t_RTE_PROC_PRIMARY, rte_ring_dequeue_real, rte_ring_lookup,
    rte_ring, rte_mempool, rte_mempool_get_real
};
use crate::Result;
use crate::Error;

use std::io;

use std::sync::mpsc::Receiver;
// use std::thread;
use std::time;

use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr::null_mut;

use logger::warn;


pub fn test_func() {
    println!("Hellooo, is this working?");
}

pub struct ClientDpdk {
    // The rust channel used to get packets from firecracker thread.
    from_firecracker: Receiver<i32>,

    // The rte rings used to send mbufs to primary app.
    receive_ring_name: CString,
    send_ring_name: CString,
    mempool_name: CString,

    receive_ring: *mut rte_ring,
    send_ring: *mut rte_ring,
    mempool: *mut rte_mempool,
}

impl ClientDpdk {
    pub fn new_with_receiver(receiver_channel: Receiver<i32>) -> ClientDpdk {

        warn!("New client has been created! Yeey!");
        ClientDpdk {
            from_firecracker: receiver_channel,
            receive_ring_name: CString::new("PRI_2_SEC").expect("Receive ring name failed.\n"),
            send_ring_name: CString::new("SEC_2_PRI").expect("Send ring name failed.\n"),
            mempool_name: CString::new("MSG_POOL").expect("Mempool name failed.\n"),
            receive_ring: null_mut(),
            send_ring: null_mut(),
            mempool: null_mut(),
        }
    }

    // fn do_rte_mempool_get(&self) -> Result<*mut c_void> {
        
    // }

    /// Receives the name of the shared ring and returns a mutable raw pointer to it.
    fn do_rte_ring_lookup(&self, ring_name: &CString) -> Result<*mut rte_ring> {

        let my_ring = unsafe { rte_ring_lookup(ring_name.as_ptr()) };
        
        if my_ring.is_null() {
            return Err(Error::RingLookupFailed);
        }

        Ok(my_ring)
    }

    /// Receives the name of the shared mempool and returns a mutable raw pointer to it.
    fn do_rte_mempool_lookup(&self, mempool_name: &CString) -> Result<*mut rte_mempool> {

        let my_mempool = unsafe { rte_mempool_lookup(mempool_name.as_ptr()) };
        
        if my_mempool.is_null() {
            return Err(Error::MempoolLookupFailed);
        }

        Ok(my_mempool)
    }

    /// Call rte_ring_lookup binding for send and receive ring
    /// Panics if any of these fails.
    fn attach_to_rings(&mut self) {

        self.receive_ring = self.do_rte_ring_lookup(&self.receive_ring_name).expect("Receive ring lookup failed");
        self.send_ring = self.do_rte_ring_lookup(&self.send_ring_name).expect("Send ring lookup failed");
    }

    /// Calls rte_mempook_lookup binding.
    /// Panics if it fails
    fn attach_to_mempool(&mut self) {

        self.mempool = self.do_rte_mempool_lookup(&self.mempool_name).expect("Mempool lookup failed");
    }


    /// Checks if DPDK Proc was started as secondary.
    /// It is mandatory.
    fn check_proc_type(&self) -> Result<()> {

        if unsafe { rte_eal_process_type() } == rte_proc_type_t_RTE_PROC_PRIMARY {
            return Err(Error::NotSecondaryDpdk);
        }
        Ok(())
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
        let ret_val = unsafe { rte_eal_init(cnt, my_args) };

        if 0 > ret_val {
            warn!("Eroare, nu a mers rte_eal_init.");
            warn!("Error message: {:?}", io::Error::last_os_error());
            // remember, error code is not inside ret_val
            // it is inside errno
            return Err(Error::EalInitFailed(ret_val));
        } else {
            warn!("Este BAAAA A MERS!!!");
            return Ok(());
        }
    }

    pub fn start_dispatcher(&mut self) {

        self.do_rte_eal_init().expect("Failled rte_eal_init call");
        warn!("rte_eal_init success");
        
        self.check_proc_type().expect("DPDK Process type should be SECONDARY: --proc-type=secondary");
        warn!("process type success");
        
        self.attach_to_rings();
        warn!("rings attached success");

        self.attach_to_mempool();
        warn!("Mempool attached success");

        let mut my_number = 0;

        loop {
            // match self.from_firecracker.recv_timeout(time::Duration::from_secs(20)) {
            //     Ok(numar) => { warn!("Received something! Number is: {}\n", numar) },
            //     Err(_) => { warn!("Nothing received.\n" )}
            // };
            
            match self.from_firecracker.recv() {
                Ok(numar) => {
                    warn!("Received something! Number is: {}\n", numar);
                    my_number = numar;
                },
                Err(_) => { warn!("Channel closed by sender. No more to receive.\n" )}
            };

            
        }
    }
}
