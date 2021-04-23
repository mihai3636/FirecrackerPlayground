use crate::bindingsMbuf::{
    rte_eal_init,
    rte_eal_process_type,
    rte_mbuf,
    rte_mempool_lookup,
    rte_proc_type_t_RTE_PROC_PRIMARY,
    rte_ring_dequeue_real,
    rte_ring_lookup,
    rte_ring,
    rte_mempool,
    rte_mempool_get_real,
    rte_ring_enqueue_real,
    rte_mempool_put_real,
    rte_pktmbuf_alloc_real,
};

use crate::Result;
use crate::Error;

use std::io;

use std::sync::mpsc::Receiver;
// use std::thread;
use std::time;

use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr::{copy, null_mut};

use logger::warn;
use crate::MAX_BUFFER_SIZE;


pub fn test_func() {
    println!("Hellooo, is this working?");
}

pub struct ClientDpdk {
    // The rust channel used to get packets from firecracker thread.
    from_firecracker: Receiver<Vec<u8>>,

    // The rte rings used to send mbufs to primary app.
    receive_ring_name: CString,
    send_ring_name: CString,
    mempool_name: CString,

    receive_ring: *mut rte_ring,
    send_ring: *mut rte_ring,
    mempool: *mut rte_mempool,
}

impl ClientDpdk {
    pub fn new_with_receiver(receiver_channel: Receiver<Vec<u8>>) -> ClientDpdk {

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


    fn print_hex_vec(&self, my_vec: &Vec<u8>) {
        let mut output = " ".to_string();
        for number in my_vec.iter() {
            output = format!("{} {:02x}", output, number);
            // warn!("{:02x} ");
        }

        warn!("{}", output);
    }

    /// UNSAFE FUNC
    /// Puts a vector inside the data buffer of mbuf structure
    /// First param: a pointer to struct rte_mbuf
    /// Second param: a pointer to your vec. It has to be Vec<u8>
    /// Size of your vector
    fn put_vec_into_buf(&self, struct_pt: *mut rte_mbuf, my_vec: *mut u8, my_vec_size: usize) {

        unsafe {
            let buf_addr: *mut c_void = (*struct_pt).buf_addr;
            let mut real_buf_addr = buf_addr.offset((*struct_pt).data_off as isize);

            copy(my_vec, real_buf_addr as *mut u8, my_vec_size);
            (*struct_pt).data_len =  my_vec_size as u16;
            (*struct_pt).pkt_len = my_vec_size as u32;
            (*struct_pt).nb_segs = 1;
        }
    }

    /// Calls the rte_ring_enqueue binding
    /// Returns error if function fails. (not enough room in the ring to enqueue)
    fn do_rte_ring_enqueue(&self, obj: *mut c_void) -> Result<()> {
        // We are going to enqueue only on the SEND ring.

        let rez = unsafe { rte_ring_enqueue_real(self.send_ring, obj) };
        if rez != 0 {
            return Err(Error::RingEnqueueFailed);
        }

        Ok(())
    }

    /// NOT TESTED
    /// Calls the rte_mempool_put binding
    /// The binding returns void so the wrapper returns nothing.
    /// No information about errors. Probably in errno?
    fn do_rte_mempool_put(&self, obj: *mut c_void) {
        unsafe { rte_mempool_put_real(self.mempool, obj) };
    }

    /// NOT TESTED
    /// Similar to mempool_get, INVESTIGATE DIFFERENCE.
    /// I'm hoping this func will eliminated the IFG bug.
    fn do_rte_pktmbuf_alloc(&self) -> Result<*mut rte_mbuf> {
        let mut my_mbuf: *mut rte_mbuf;

        my_mbuf = unsafe { rte_pktmbuf_alloc_real(self.mempool) };
        if my_mbuf.is_null() {
            return Err(Error::PktmbufAllocFailed);
        }

        Ok(my_mbuf)
    }

    /// Calls the rte_mempool_get binding
    /// Returns address of mempool buffer /object?
    /// Returns error if function fails. (no object available from mempool)
    fn do_rte_mempool_get(&self) -> Result<*mut c_void> {
        let mut my_buffer: *mut c_void = null_mut();
        let my_buffer_addr: *mut *mut c_void = &mut my_buffer;

        let rez = unsafe { rte_mempool_get_real(self.mempool, my_buffer_addr) };
        if 0 > rez {
            return Err(Error::MempoolGetFailed);
        }

        Ok(my_buffer)
    }

    /// Calls the rte_ring_lookup binding.
    /// Receives the name of the shared ring and returns a mutable raw pointer to it.
    fn do_rte_ring_lookup(&self, ring_name: &CString) -> Result<*mut rte_ring> {

        let my_ring = unsafe { rte_ring_lookup(ring_name.as_ptr()) };
        
        if my_ring.is_null() {
            return Err(Error::RingLookupFailed);
        }

        Ok(my_ring)
    }

    /// Calls the rte_mempool_lookup binding
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

        let mut my_data: Vec<u8> = Vec::new(); 

        loop {
            // match self.from_firecracker.recv_timeout(time::Duration::from_secs(20)) {
            //     Ok(numar) => { warn!("Received something! Number is: {}\n", numar) },
            //     Err(_) => { warn!("Nothing received.\n" )}
            // };
            
            match self.from_firecracker.recv() {
                Ok(some_data) => {
                    // warn!("Received something! Number is: {}\n", numar);
                    // warn!("Received the slice!");
                    my_data = some_data;
                    self.print_hex_vec(&my_data);
                    // warn!("{:?}", my_data);
                    // warn!("Length of received data in thread: {}", my_data.len());
                },
                Err(_) => { warn!("Channel closed by sender. No more to receive." )}
            };

            // After receiving something on the channel
            // I want to send it to the primary DPDK
            // And the primary will send it to hardware NIC
            
            // let mut my_mbuf = self.do_rte_mempool_get();
            // while let Err(er) = my_mbuf {
            //     warn!("rte_mempool_get failed, trying again.");
            //     my_mbuf = self.do_rte_mempool_get();
            //     // it may fail if not enough entries are available.
            // }

            let mut my_mbuf = self.do_rte_pktmbuf_alloc();
            while let Err(er) = my_mbuf {
                warn!("rte_pktmbuf_alloc failed, trying again.");
                my_mbuf = self.do_rte_pktmbuf_alloc();
            }
    
            // Let's just send an empty packet for starters.

            // To uncomment
            // let my_mbuf = my_mbuf.unwrap();
            
            // let my_mbuf_struct: *mut rte_mbuf = my_mbuf as (*mut rte_mbuf);
            let my_mbuf_struct = my_mbuf.unwrap();
            
            // let mut test_vec: Vec<u8> = vec![0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8, 0x9, 0xa, 0xb, 0xc, 0xd, 0xe, 0xf];
            self.put_vec_into_buf(my_mbuf_struct, my_data.as_mut_ptr(), my_data.len());

            // self.put_vec_into_buf(my_mbuf_struct, test_vec.as_mut_ptr(), test_vec.len());

            let mut res = self.do_rte_ring_enqueue(my_mbuf_struct as *mut c_void);
            // it may fail if not enough room in the ring to enqueue
            while let Err(er) = res {
                warn!("rte_ring_enqueue failed, trying again.");
                res = self.do_rte_ring_enqueue(my_mbuf_struct as *mut c_void);
            }
            warn!("rte_ring_enqueue success");
        }
    }
}

// unsafe {
//     warn!("Length of segment buffer: {}", (*my_buffer_struct).buf_len);
//     warn!("Data offset: {}", (*my_buffer_struct).data_off);
//     let buf_addr: *mut c_void = (*my_buffer_struct).buf_addr;
//     let real_buf_addr = buf_addr.offset((*my_buffer_struct).data_off as isize);
//     warn!("Address of buf_addr: {:?}", buf_addr);
//     warn!("Address of buf_addr + data_off: {:?}", real_buf_addr);
//     warn!("\n");
// };
