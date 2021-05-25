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
    rte_ring_empty_real,
    rte_ring_enqueue_burst_real,
    rte_ring_dequeue_burst_real,
};

use utils::eventfd::EventFd;

use crate::Result;
use crate::Error;

use std::io;

use std::sync::mpsc::{Receiver, channel, TryRecvError, Sender};
use std::time;

use std::ffi::CString;
use std::os::raw::{c_void, c_uint};
use std::ptr::{copy, null_mut, copy_nonoverlapping};

use logger::{warn, error};
use crate::MAX_BUFFER_SIZE;
use crate::ArrayTuple;
use std::time::Duration;


pub fn test_func() {
    println!("Hellooo, is this working?");
}

pub struct ClientDpdk {

    // The rte rings used to send mbufs to primary app.
    receive_ring_name: CString,
    send_ring_name: CString,
    mempool_name: CString,

    receive_ring: *mut rte_ring,
    send_ring: *mut rte_ring,
    mempool: *mut rte_mempool,

    event_dpdk_secondary: EventFd,
}
unsafe impl Send for ClientDpdk {}
unsafe impl Sync for ClientDpdk {}

impl ClientDpdk {
    pub fn new_with_receiver(
        event_dpdk_secondary: EventFd,
    ) -> ClientDpdk {

        warn!("New client has been created! Yeey!");
        let mut client = ClientDpdk {
            receive_ring_name: CString::new("PRI_2_SEC").expect("Receive ring name failed.\n"),
            send_ring_name: CString::new("SEC_2_PRI").expect("Send ring name failed.\n"),
            mempool_name: CString::new("MSG_POOL").expect("Mempool name failed.\n"),
            receive_ring: null_mut(),
            send_ring: null_mut(),
            mempool: null_mut(),
            event_dpdk_secondary: event_dpdk_secondary,
        };

        // Calling the init functions.
        // I do this here so I won't have to use mutable self functions on the object.
        // This way I can hold a reference shared across threads.

        client.do_rte_eal_init().expect("Failled rte_eal_init call");
        warn!("rte_eal_init success");

        client.check_proc_type().expect("DPDK Process type should be SECONDARY: --proc-type=secondary");
        warn!("process type success");

        client.attach_to_rings();
        warn!("rings attached success");

        client.attach_to_mempool();
        warn!("Mempool attached success");

        client
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
    fn put_vec_into_mbuf(&self, struct_pt: *mut rte_mbuf, my_vec: *mut u8, my_vec_size: usize) {

        unsafe {
            let buf_addr: *mut c_void = (*struct_pt).buf_addr;
            let mut real_buf_addr = buf_addr.offset((*struct_pt).data_off as isize);
            // warn!("Trying to put vec into mbuf, size: {}", my_vec_size);

            copy_nonoverlapping(my_vec, real_buf_addr as *mut u8, my_vec_size);
            (*struct_pt).data_len =  my_vec_size as u16;
            (*struct_pt).pkt_len = my_vec_size as u32;
            (*struct_pt).nb_segs = 1;
        }
    }

    /// UNSAFE FUNC
    /// Puts an array inside the data buffer of mbuf structure
    /// First param: a pointer to struct rte_mbuf
    /// Second param: a pointer to your array. It has to be [u8; MAX_BUFFER_SIZE]
    /// Third param: size of your vector
    fn put_array_into_mbuf(&self, struct_pt: *mut rte_mbuf, my_array: *const u8, my_array_size: usize) {

        unsafe {
            let buf_addr: *mut c_void = (*struct_pt).buf_addr;
            let mut real_buf_addr = buf_addr.offset((*struct_pt).data_off as isize);
            // warn!("Trying to put vec into mbuf, size: {}", my_vec_size);

            copy_nonoverlapping(my_array, real_buf_addr as *mut u8, my_array_size);
            (*struct_pt).data_len =  my_array_size as u16;
            (*struct_pt).pkt_len = my_array_size as u32;
            (*struct_pt).nb_segs = 1;
        }
    }

    /// UNSAFE FUNC
    /// Receives am mbuf struct as param.
    /// Copies its data buffer into a Vec<u8>
    /// Returns the new Vec<u8>
    fn get_vec_from_mbuf(&self, struct_pt: *mut c_void) -> Vec<u8> {

        let mbuf_ptr: *mut rte_mbuf;
        let mut rez_vec: Vec<u8>;
        let mut data_buf_size: usize;

        mbuf_ptr = struct_pt as *mut rte_mbuf;
        unsafe {
            let mut data_buf_addr =(*mbuf_ptr).buf_addr;
            let mut real_data_buf_addr = data_buf_addr.offset((*mbuf_ptr).data_off as isize);

            data_buf_size = (*mbuf_ptr).data_len as usize;            
            rez_vec = Vec::with_capacity(data_buf_size);
            copy_nonoverlapping(real_data_buf_addr as *const u8, rez_vec.as_mut_ptr() as *mut u8, data_buf_size);
            // Very important to set len!
            rez_vec.set_len(data_buf_size);
        }

        rez_vec
    }

    /// UNSAFE FUNC
    /// Receives am mbuf struct as param and a pointer to an array
    /// Copies mbuf data into the array, startin from 12th position
    /// Returns the size of the data
    fn get_array_from_mbuf(&self, struct_pt: *mut c_void, buf_array: &mut [u8; MAX_BUFFER_SIZE]) -> usize {

        let mbuf_ptr: *mut rte_mbuf;
        let mut data_buf_size: usize;

        mbuf_ptr = struct_pt as *mut rte_mbuf;
        unsafe {
            let mut data_buf_addr =(*mbuf_ptr).buf_addr;
            let mut real_data_buf_addr = data_buf_addr.offset((*mbuf_ptr).data_off as isize);

            // I have to put the data starting from 12th index, because of the vnet hdr.
            let mut buf_array_offset = &mut buf_array[0] as *mut u8;
            buf_array_offset = buf_array_offset.offset(12);

            data_buf_size = (*mbuf_ptr).data_len as usize;            
            copy_nonoverlapping(real_data_buf_addr as *const u8, buf_array_offset, data_buf_size);
        }

        data_buf_size
    }

    // Returns true if the given ring is empty
    // Returns false if the ring is not empty
    pub fn do_rte_ring_empty(&self, pt_ring: *mut rte_ring) -> bool {
        let rez: i32 = unsafe { rte_ring_empty_real(self.receive_ring) };
        if rez == 1 {
            return true;
        }
        false
    }

    pub fn do_rte_ring_dequeue(&self, obj_p: *mut *mut c_void) -> Result<()> {

        let rez = unsafe { rte_ring_dequeue_real(self.receive_ring, obj_p) };
        // Returns

        // 0: Success, objects dequeued.
        // -ENOENT: Not enough entries in the ring to dequeue, no object is dequeued.
        if 0 > rez {
            return Err(Error::RingDequeueFailed);
        }

        Ok(())
    }

    /// Blocks untill all buffer has been enqueued.
    /// First param: address of an array containings mbufs
    /// Second param: number of mbufs in the array
    /// Third param can be NULL, check DPDK docs.
    pub fn enqueue_burst_untill_done(
        &self,
        obj_table: *mut *mut c_void,
        n: c_uint,
        free_space: *mut c_uint
    ) {
        let mut current_index = 0;
        let mut so_far = 0;
        let total = n;
        let mut nr_to_enq = n;
        let mut pointer_to_mbuf_pointers = obj_table;

        while so_far != total {
            let rez = self.do_rte_ring_enqueue_burst(pointer_to_mbuf_pointers, nr_to_enq, null_mut()).unwrap();
            so_far += rez;
            // There are less elements to enq now.
            nr_to_enq = nr_to_enq - rez;
            pointer_to_mbuf_pointers = unsafe { pointer_to_mbuf_pointers.offset(rez as isize) };

            if so_far != total {
                warn!("Inside loop");
            }
        }
    }

    /// Calls the rte_ring_enqueue_burst_real binding
    /// First param: address of an array containings mbufs
    /// Second param: number of mbufs in the array
    /// Third param can be NULL, check DPDK docs.
    /// Returns number of mbufs successfully inserted.
    pub fn do_rte_ring_enqueue_burst(
        &self,
        obj_table: *mut *mut c_void,
        n: c_uint,
        free_space: *mut c_uint
    ) -> Result<u32> {

        let rez = unsafe { rte_ring_enqueue_burst_real(self.send_ring, obj_table, n, free_space) };
        // No error code returned
        Ok(rez)
    }

    /// Calls the rte_ring_dequeue_burst_real binding
    /// First param: address of an array containings mbufs
    /// Second param: Maximum number of mbufs it is capable to dequeue.
    /// Third param can be NULL, check DPDK docs.
    /// Returns number of mbufs successfully inserted.
    pub fn do_rte_ring_dequeue_burst(
        &self,
        obj_table: *mut *mut c_void,
        n: c_uint,
        available: *mut c_uint,
    ) -> Result<u32> {
        let rez = unsafe { rte_ring_dequeue_burst_real(self.send_ring, obj_table, n, available) };
        // No error code is returned
        Ok(rez)
    }

    /// Calls the rte_ring_enqueue binding
    /// Returns error if function fails. (not enough room in the ring to enqueue)
    pub fn do_rte_ring_enqueue(&self, obj: *mut c_void) -> Result<()> {
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
    pub fn do_rte_mempool_put(&self, obj: *mut c_void) {
        unsafe { rte_mempool_put_real(self.mempool, obj) };
    }

    /// Calls the rte_mempool_get binding
    /// Returns address of mempool buffer /object?
    /// Returns error if function fails. (no object available from mempool)
    pub fn do_rte_mempool_get(&self) -> Result<*mut c_void> {
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

    /// Receives a Vec<u8> as param which represent the packet.
    /// Puts the packet inside an mbuf
    /// Then puts the mbuf on a shared ring
    fn send_packet_to_primary(&self, my_data: &mut ArrayTuple) {
        
        let mut my_mbuf = self.do_rte_mempool_get();
        while let Err(er) = my_mbuf {
            warn!("rte_mempool_get failed, trying again.");
            my_mbuf = self.do_rte_mempool_get();
            // it may fail if not enough entries are available.
        }

        // Put the packet into the mbuf
        let my_mbuf = my_mbuf.unwrap();
        let my_mbuf_struct: *mut rte_mbuf = my_mbuf as (*mut rte_mbuf);

        // self.put_vec_into_mbuf(my_mbuf_struct, my_data.as_mut_ptr(), my_data.len());

        let mut my_array: &mut [u8; MAX_BUFFER_SIZE] = &mut my_data.0;
        let length: usize = my_data.1 - 12;

        self.put_array_into_mbuf(my_mbuf_struct, &mut my_array[12] as *const u8, length);
        // let mut test_vec: Vec<u8> = vec![0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8, 0x9, 0xa, 0xb, 0xc, 0xd, 0xe, 0xf];
        // self.put_vec_into_buf(my_mbuf_struct, test_vec.as_mut_ptr(), test_vec.len());

        // Now we put the mbuf into the shared ring
        // So the primary will get it.
        let mut res = self.do_rte_ring_enqueue(my_mbuf);
        // it may fail if not enough room in the ring to enqueue
        while let Err(er) = res {
            warn!("rte_ring_enqueue failed, trying again.");
            res = self.do_rte_ring_enqueue(my_mbuf);
        }
        // warn!("ENQUEUE success");
    }

    /// Each time something appears on the queue, it will signal Fc
    /// to start the receive routine.
    pub fn start_dispatcher(&self) {

        loop {
            // We are breaking the loop if Firecracker thread kills the channel.

            // Getting mbuf from shared ring
            if false == self.do_rte_ring_empty(self.receive_ring) {
                // Enters here only if mbuf was waiting in the queue
                self.event_dpdk_secondary.write(1).map_err(|e| {
                    error!("Failed to signal the Net device from DpdkClient {:?}", e);
                });
            }
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
