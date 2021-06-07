// Copyright 2020 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Portions Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the THIRD-PARTY file.

use crate::virtio::net::tap::Tap;
#[cfg(test)]
use crate::virtio::net::test_utils::Mocks;
use crate::virtio::net::Error;
use crate::virtio::net::Result;
use crate::virtio::net::{MAX_BUFFER_SIZE, QUEUE_SIZE, QUEUE_SIZES, RX_INDEX, TX_INDEX};
use crate::virtio::{
    ActivateResult, DeviceState, Queue, VirtioDevice, TYPE_NET, VIRTIO_MMIO_INT_VRING,
};
use crate::{report_net_event_fail, Error as DeviceError};
//Removed by Mihai
// use dumbo::pdu::ethernet::EthernetFrame;
use libc::EAGAIN;
use logger::{error, warn, IncMetric, METRICS};
use mmds::ns::MmdsNetworkStack;
use rate_limiter::{BucketUpdate, RateLimiter, TokenType};
#[cfg(not(test))]
use std::io;
use std::io::{Read, Write};
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::{cmp, mem, result};
use utils::eventfd::EventFd;
use utils::net::mac::{MacAddr, MAC_ADDR_LEN};
use virtio_gen::virtio_net::{
    virtio_net_hdr_v1, VIRTIO_F_VERSION_1, VIRTIO_NET_F_CSUM, VIRTIO_NET_F_GUEST_CSUM,
    VIRTIO_NET_F_GUEST_TSO4, VIRTIO_NET_F_GUEST_UFO, VIRTIO_NET_F_HOST_TSO4, VIRTIO_NET_F_HOST_UFO,
    VIRTIO_NET_F_MAC,
};
use vm_memory::{ByteValued, Bytes, GuestAddress, GuestMemoryError, GuestMemoryMmap};

// added by Mihai
use std::sync::mpsc::{Sender, Receiver, TryRecvError};
use std::sync::mpsc;
use std::boxed::Box;
use std::option;
use std::time::Duration;
use std::time::Instant;

use dpdk_component::client::{
    ClientDpdk,
};

use dpdk_component::bindingsMbuf::{
    rte_mbuf,
    PKT_TX_TCP_CKSUM,
};

use std::ptr::{
    null_mut,
    copy_nonoverlapping,
};

use std::ffi::c_void;

use crate::virtio::net::{ArrayTuple, ARRAY_MBUFS};

enum FrontendError {
    AddUsed,
    DescriptorChainTooSmall,
    EmptyQueue,
    GuestMemory(GuestMemoryError),
    ReadOnlyDescriptor,
}

/// Added by Mihai
/// COpy of func from DpdkClient
/// TO DELETE
fn print_hex_vec(my_vec: &Vec<u8>) {
    let mut output = " ".to_string();
    for number in my_vec.iter() {
        output = format!("{} {:02x}", output, number);
        // warn!("{:02x} ");
    }

    warn!("{}", output);
}

pub(crate) fn vnet_hdr_len() -> usize {
    mem::size_of::<virtio_net_hdr_v1>()
}

// Frames being sent/received through the network device model have a VNET header. This
// function returns a slice which holds the L2 frame bytes without this header.
fn frame_bytes_from_buf(buf: &[u8]) -> Result<&[u8]> {
    if buf.len() < vnet_hdr_len() {
        Err(Error::VnetHeaderMissing)
    } else {
        Ok(&buf[vnet_hdr_len()..])
    }
}

fn frame_bytes_from_buf_mut(buf: &mut [u8]) -> Result<&mut [u8]> {
    if buf.len() < vnet_hdr_len() {
        Err(Error::VnetHeaderMissing)
    } else {
        Ok(&mut buf[vnet_hdr_len()..])
    }
}

// This initializes to all 0 the VNET hdr part of a buf.
fn init_vnet_hdr(buf: &mut [u8]) {
    // The buffer should be larger than vnet_hdr_len.
    // TODO: any better way to set all these bytes to 0? Or is this optimized by the compiler?
    for i in &mut buf[0..vnet_hdr_len()] {
        *i = 0;
    }
}

#[derive(Clone, Copy)]
pub struct ConfigSpace {
    pub guest_mac: [u8; MAC_ADDR_LEN],
}

impl Default for ConfigSpace {
    fn default() -> ConfigSpace {
        ConfigSpace {
            guest_mac: [0; MAC_ADDR_LEN],
        }
    }
}

unsafe impl ByteValued for ConfigSpace {}

pub struct Net {
    pub(crate) id: String,

    pub(crate) tap: Tap,

    pub(crate) avail_features: u64,
    pub(crate) acked_features: u64,

    pub(crate) queues: Vec<Queue>,
    pub(crate) queue_evts: Vec<EventFd>,

    pub(crate) rx_rate_limiter: RateLimiter,
    pub(crate) tx_rate_limiter: RateLimiter,

    pub(crate) rx_deferred_frame: bool,
    rx_deferred_irqs: bool,

    rx_bytes_read: usize,
    rx_frame_buf: [u8; MAX_BUFFER_SIZE],

    tx_iovec: Vec<(GuestAddress, usize)>,
    tx_frame_buf: [u8; MAX_BUFFER_SIZE],

    pub(crate) interrupt_status: Arc<AtomicUsize>,
    pub(crate) interrupt_evt: EventFd,

    pub(crate) config_space: ConfigSpace,
    pub(crate) guest_mac: Option<MacAddr>,

    pub(crate) device_state: DeviceState,
    pub(crate) activate_evt: EventFd,

    pub(crate) mmds_ns: Option<MmdsNetworkStack>,

    #[cfg(test)]
    pub(crate) mocks: Mocks,

    // Added by Mihai
    //Using this eventFd to know when secondary dpdk has data to send to net device.
    pub(crate) event_secondary_dpdk: EventFd,
    pub(crate) client: Arc<ClientDpdk>,
    //Using these two to dequeue burst received packets from RX RING DPDK
    array_mbuf: [*mut rte_mbuf; ARRAY_MBUFS],
    index_mbuf: usize,
    size_array: usize,
    last_tx: Instant,
    total_tx: usize,

}
unsafe impl Send for Net {}
impl Net {
    /// Create a new virtio network device with the given TAP interface.
    pub fn new_with_tap(
        id: String,
        tap_if_name: String,
        guest_mac: Option<&MacAddr>,
        rx_rate_limiter: RateLimiter,
        tx_rate_limiter: RateLimiter,
        allow_mmds_requests: bool,
    ) -> Result<Self> {
        let tap = Tap::open_named(&tap_if_name).map_err(Error::TapOpen)?;

        // Set offload flags to match the virtio features below.
        tap.set_offload(
            net_gen::TUN_F_CSUM | net_gen::TUN_F_UFO | net_gen::TUN_F_TSO4 | net_gen::TUN_F_TSO6,
        )
        .map_err(Error::TapSetOffload)?;

        let vnet_hdr_size = vnet_hdr_len() as i32;
        tap.set_vnet_hdr_size(vnet_hdr_size)
            .map_err(Error::TapSetVnetHdrSize)?;

        let mut avail_features = 1 << VIRTIO_NET_F_GUEST_CSUM
            | 1 << VIRTIO_NET_F_CSUM
            | 1 << VIRTIO_NET_F_GUEST_TSO4
            | 1 << VIRTIO_NET_F_GUEST_UFO
            | 1 << VIRTIO_NET_F_HOST_TSO4
            | 1 << VIRTIO_NET_F_HOST_UFO
            | 1 << VIRTIO_F_VERSION_1;

        let mut config_space = ConfigSpace::default();
        if let Some(mac) = guest_mac {
            config_space.guest_mac.copy_from_slice(mac.get_bytes());
            // When this feature isn't available, the driver generates a random MAC address.
            // Otherwise, it should attempt to read the device MAC address from the config space.
            avail_features |= 1 << VIRTIO_NET_F_MAC;
        }

        let mut queue_evts = Vec::new();
        for _ in QUEUE_SIZES.iter() {
            queue_evts.push(EventFd::new(libc::EFD_NONBLOCK).map_err(Error::EventFd)?);
        }

        let queues = QUEUE_SIZES.iter().map(|&s| Queue::new(s)).collect();

        let mmds_ns = if allow_mmds_requests {
            Some(MmdsNetworkStack::new_with_defaults(None))
        } else {
            None
        };

        let event_secondary_dpdk = EventFd::new(libc::EFD_NONBLOCK).map_err(Error::EventFd)?;

        let event_backup = event_secondary_dpdk.try_clone().expect("Couldn't duplicate the eventfd_dpdk_secondary!");

        let cl = ClientDpdk::new_with_receiver(event_backup);
        let client = Arc::new(cl);
        let client_clone = client.clone();

        // Added by Mihai
        std::thread::spawn(move || {
            client_clone.start_dispatcher()
        });

        Ok(Net {
            id,
            tap,
            avail_features,
            acked_features: 0u64,
            queues,
            queue_evts,
            rx_rate_limiter,
            tx_rate_limiter,
            rx_deferred_frame: false,
            rx_deferred_irqs: false,
            rx_bytes_read: 0,
            rx_frame_buf: [0u8; MAX_BUFFER_SIZE],
            tx_frame_buf: [0u8; MAX_BUFFER_SIZE],
            tx_iovec: Vec::with_capacity(QUEUE_SIZE as usize),
            interrupt_status: Arc::new(AtomicUsize::new(0)),
            interrupt_evt: EventFd::new(libc::EFD_NONBLOCK).map_err(Error::EventFd)?,
            device_state: DeviceState::Inactive,
            activate_evt: EventFd::new(libc::EFD_NONBLOCK).map_err(Error::EventFd)?,
            config_space,
            mmds_ns,
            guest_mac: guest_mac.copied(),

            #[cfg(test)]
            mocks: Mocks::default(),
            event_secondary_dpdk: event_secondary_dpdk,
            client: client,
            array_mbuf: [null_mut(); ARRAY_MBUFS],
            index_mbuf: 0,
            size_array: 0,
            last_tx: Instant::now(),
            total_tx: 0,
        })
    }

    /// Added by Mihai
    /// Checking if still receiving interrupts on tap.
    pub fn tap_dummy_handler(&self) {
        warn!("This should no longer be called AAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n");
    }

    /// Added by Mihai
    /// Dummy func to check if triggering events is working.
    pub fn dpdk_secondary_handler(&self) {
        warn!("Yo yo yo, the DPDK secondary TRIGGERED the event y'all been waiting for.");
    }

    /// Provides the ID of this net device.
    pub fn id(&self) -> &String {
        &self.id
    }

    /// Provides the MAC of this net device.
    pub fn guest_mac(&self) -> Option<&MacAddr> {
        self.guest_mac.as_ref()
    }

    /// Provides a mutable reference to the `MmdsNetworkStack`.
    pub fn mmds_ns_mut(&mut self) -> Option<&mut MmdsNetworkStack> {
        self.mmds_ns.as_mut()
    }

    fn signal_used_queue(&mut self) -> result::Result<(), DeviceError> {
        self.interrupt_status
            .fetch_or(VIRTIO_MMIO_INT_VRING as usize, Ordering::SeqCst);
        self.interrupt_evt.write(1).map_err(|e| {
            error!("Failed to signal used queue: {:?}", e);
            METRICS.net.event_fails.inc();
            DeviceError::FailedSignalingUsedQueue(e)
        })?;

        self.rx_deferred_irqs = false;
        Ok(())
    }

    fn signal_rx_used_queue(&mut self) -> result::Result<(), DeviceError> {

        // Info added by Mihai
        // This flag is set to true after successfully writing the packet on Guest at least once.
        if self.rx_deferred_irqs {
            return self.signal_used_queue();
        }

        Ok(())
    }

    // Info by Mihai: confusing func name, it copies packet to guest memory!
    // Attempts to copy a single frame into the guest if there is enough
    // rate limiting budget.
    // Returns true on successful frame delivery.
    fn rate_limited_rx_single_frame(&mut self) -> bool {
        

        // Attempt frame delivery.
        // Info by Mihai
        // If return is ok, then self.rx_deferred_irqs is set to TRUE.
        let success = self.write_frame_to_guest();

        // Info by Mihai
        // Only way this could return FALSE is if write_frame_to_guest fails with
        // Empty Queue or Add Used Error!
        success
    }

    // Copies a single frame from `self.rx_frame_buf` into the guest.
    fn do_write_frame_to_guest(&mut self) -> std::result::Result<(), FrontendError> {
        let mut result: std::result::Result<(), FrontendError> = Ok(());
        let mem = match self.device_state {
            DeviceState::Activated(ref mem) => mem,
            // This should never happen, it's been already validated in the event handler.
            DeviceState::Inactive => unreachable!(),
        };

        let queue = &mut self.queues[RX_INDEX];
        let head_descriptor = queue.pop(mem).ok_or_else(|| {
            METRICS.net.no_rx_avail_buffer.inc();
            FrontendError::EmptyQueue
        })?;
        let head_index = head_descriptor.index;

        // Get the mbuf from the array
        let my_mbuf_pt: *mut rte_mbuf = self.array_mbuf[self.index_mbuf];
        // unsafe { warn!("{}", (*my_mbuf_pt).data_len)};
        
        // rte_pktmbuf_prepend vnet_header_size in mbuf data
        let mut mbuf_data: *mut u8 = self.client.do_rte_pktmbuf_prepend(my_mbuf_pt, vnet_hdr_len() as u16).unwrap();
    
        // Add the vnet header
        let mut mbuf_data_slice: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(mbuf_data, (*my_mbuf_pt).data_len as usize) };
        init_vnet_hdr(mbuf_data_slice);
        // The frame slice that we write to guest's memory == the data from the mbuf + vnet header
        let mut frame_slice =  mbuf_data_slice as &[u8];
        let frame_len = frame_slice.len();

        let mut maybe_next_descriptor = Some(head_descriptor);
        while let Some(descriptor) = &maybe_next_descriptor {
            if frame_slice.is_empty() {
                break;
            }

            if !descriptor.is_write_only() {
                result = Err(FrontendError::ReadOnlyDescriptor);
                break;
            }

            let len = std::cmp::min(frame_slice.len(), descriptor.len as usize);
            match mem.write_slice(&frame_slice[..len], descriptor.addr) {
                Ok(()) => {
                    METRICS.net.rx_count.inc();
                    frame_slice = &frame_slice[len..];
                }
                Err(e) => {
                    error!("Failed to write slice: {:?}", e);
                    match e {
                        GuestMemoryError::PartialBuffer { .. } => &METRICS.net.rx_partial_writes,
                        _ => &METRICS.net.rx_fails,
                    }
                    .inc();
                    result = Err(FrontendError::GuestMemory(e));
                    break;
                }
            };

            maybe_next_descriptor = descriptor.next_descriptor();
        }

        if result.is_ok() && !frame_slice.is_empty() {
            warn!("Receiving buffer is too small to hold frame of current size");
            METRICS.net.rx_fails.inc();
            result = Err(FrontendError::DescriptorChainTooSmall);
        }

        // Mark the descriptor chain as used. If an error occurred, skip the descriptor chain.
        let used_len = if result.is_err() { 0 } else { frame_len as u32 };
        queue.add_used(mem, head_index, used_len).map_err(|e| {
            error!("Failed to add available descriptor {}: {}", head_index, e);
            FrontendError::AddUsed
        })?;
        self.rx_deferred_irqs = true;

        if result.is_ok() {
            METRICS.net.rx_bytes_count.add(frame_len);
            METRICS.net.rx_packets_count.inc();
            // It is not modulo  ARRAY_MBUFS
            // It should be modulo number of the mbufs read originally.
            self.index_mbuf = self.index_mbuf + 1;
            self.size_array = self.size_array - 1;

            if self.size_array == 0 {
                self.index_mbuf = 0;
            }

            // Idea: maybe do bunch put somehow?
            self.client.do_rte_mempool_put(my_mbuf_pt as *mut c_void);
            // warn!("Wrote on guest mem");
        }

        // TIPS IF U IMPLEMENT BY SAVING MBUF INTO VNET
        // SO far u did not save the mbuf and did an extra copy.
        //Advantage: u release the mbuf quicker

        // Warning, trebuie avut grija la cazurile cu erori pe EmptyQueue sau AddUsed

        // Daca ai AddUsed, pui pachetul in rx_frame_buf si arunci mbuf-ul.
        // Data viitoare cand vrei sa scrii, verifici daca ai deferred frame
        // SI daca ai avut deferred frame atunci copiezi din rx_frame_buf in guest, nu din mbuf in guest.

        // Vezi ca self.deferred_frame e resetat inainte sa intri iar in functia asta!

        // TO DO: dequeue mbuf from receive ring, write the mbuf inside the guest, free the mbuf.
        // Don't forget to add vnet_header before writing it to guest.

        result
    }

    // Copies a single frame from `self.rx_frame_buf` into the guest. In case of an error retries
    // the operation if possible. Returns true if the operation was successfull.
    fn write_frame_to_guest(&mut self) -> bool {
        let max_iterations = self.queues[RX_INDEX].actual_size();
        for _ in 0..max_iterations {
            match self.do_write_frame_to_guest() {
                Ok(()) => return true,
                Err(FrontendError::EmptyQueue) | Err(FrontendError::AddUsed) => {
                    return false;
                }
                Err(_) => {
                    // retry
                    continue;
                }
            }
        }

        false
    }

    // Tries to detour the frame to MMDS and if MMDS doesn't accept it, sends it on the host TAP.
    //
    // `frame_buf` should contain the frame bytes in a slice of exact length.
    // Returns whether MMDS consumed the frame.
    fn write_to_mmds_or_tap(
        mmds_ns: Option<&mut MmdsNetworkStack>,
        rate_limiter: &mut RateLimiter,
        frame_buf: &[u8],
        tap: &mut Tap,
        guest_mac: Option<MacAddr>,
    ) -> Result<bool> {
        let checked_frame = |frame_buf| {
            frame_bytes_from_buf(frame_buf).map_err(|e| {
                error!("VNET header missing in the TX frame.");
                METRICS.net.tx_malformed_frames.inc();
                e
            })
        };
        if let Some(ns) = mmds_ns {
            if ns.detour_frame(checked_frame(frame_buf)?) {
                METRICS.mmds.rx_accepted.inc();

                // MMDS frames are not accounted by the rate limiter.
                rate_limiter.manual_replenish(frame_buf.len() as u64, TokenType::Bytes);
                rate_limiter.manual_replenish(1, TokenType::Ops);

                // MMDS consumed the frame.
                return Ok(true);
            }
        }

        // This frame goes to the TAP.


        // Removed by Mihai
        // Check for guest MAC spoofing.

        // if let Some(mac) = guest_mac {
        //     let _ = EthernetFrame::from_bytes(checked_frame(frame_buf)?).map(|eth_frame| {
        //         if mac != eth_frame.src_mac() {
        //             METRICS.net.tx_spoofed_mac_count.inc();
        //         }
        //     });
        // }

        // warn!("{:?}", frame_buf);

        // now I need to send this frame_buf to my DPDK Client!
        // so let's create the client
        match tap.write(frame_buf) {
            Ok(_) => {
                METRICS.net.tx_bytes_count.add(frame_buf.len());
                METRICS.net.tx_packets_count.inc();
                METRICS.net.tx_count.inc();
            }
            Err(e) => {
                error!("Failed to write to tap: {:?}", e);
                METRICS.net.tap_write_fails.inc();
            }
        };
        Ok(false)
    }

    /// Added by Mihai
    /// Replacement for read_from_mmds_or_tap
    /// Reads the Vec<u8> from secondary using the Secondary to FC channel.
    fn read_from_secondary(&mut self) -> result::Result<usize, DeviceError> {

        // TO DO
        // Trebuie sa pastrezi mbuf-ul in Net, pana il trimiti in functia urmatoare.

        // Get an mbuf from the ring
        let mut mbuf: *mut c_void = null_mut();
        let mbuf_ptr: *mut *mut c_void = &mut mbuf;

        let res = self.client.do_rte_ring_dequeue(mbuf_ptr);
        if let Err(_er) = res {
            // There was nothing in the ring.
            // warn!("DEQ FAILED");
            return Err(DeviceError::SecondaryEmpty);
        }

        init_vnet_hdr(&mut self.rx_frame_buf[..MAX_BUFFER_SIZE]);

        // Copy data from mbuf into rx_frame_buf
        // Possible improvement: Do not copy, hold the mbuf into Net device.
        // But you have to check deferred frame corner case, when you cannot write
        // directly into guest.
        let read_count;
    
        unsafe {
            let mbuf = mbuf as *mut rte_mbuf;
            let data_addr = (*mbuf).buf_addr;
            let real_data_addr = data_addr.offset((*mbuf).data_off as isize);

            let dest = &mut self.rx_frame_buf[vnet_hdr_len()] as *mut u8;
            let size = (*mbuf).data_len as usize;

            // copy from mbuf into rx_frame_buf on 12th pos
            copy_nonoverlapping(real_data_addr as *const u8, dest, size);
            read_count = size + vnet_hdr_len();
        }

        // Because I am copying mbuf inside the rx_frame_buf, I can release the mbuf now.
        self.client.do_rte_mempool_put(mbuf);

        Ok(read_count)
        // match self.rx_channel.try_recv() {
        //     Ok(mut some_data) => {
        //         // some_data is Box<ArrayTuple>
        //         let length = some_data.1;

        //         // warn!("Reading from SECONDARY. Size: {}", length);
        //         // init the vnet header first
        //         init_vnet_hdr(&mut some_data.0);

        //         self.rx_box.replace(some_data);

        //         // I am going to send the box back after firecracker writes this box into guest memory
        //         return Ok(length + vnet_hdr_len());
        //     },
        //     Err(TryRecvError::Disconnected) => {
        //         warn!("Secondary to Firecracker channel has been closed by Secondary. ERROR" );
        //         return Err(DeviceError::SecondaryClosed);
        //     },
        //     Err(TryRecvError::Empty) => {
        //         // warn!("Reading from SECONDARY EMPTY");
        //         return Err(DeviceError::SecondaryEmpty);
        //     },
        // };
    }

    /// Added by Mihai
    /// Replacement for read_from_secondary
    /// Reads a burst from the shared rings and stores it into array_mbuf
    /// If array_mbuf already has something, return ok
    /// If dequeue burst gives nothing, return err(empty)
    fn read_from_secondary_burst(&mut self) -> result::Result<usize, DeviceError> {

        // Very important, if array mbuf is not empty, just return ok.
        // So you dequeue burst only after u fully wrote the array.
        // Idea: add a secondary array
        if self.size_array != 0 {
            return Ok(1);
        }
        let mut pt_to_array: *mut *mut c_void = self.array_mbuf.as_mut_ptr() as *mut *mut c_void;
        // let mut adr_pt_to_array = &mut pt_to_array as *mut *mut c_void; 
        let count = self.client.do_rte_ring_dequeue_burst(pt_to_array, ARRAY_MBUFS as u32, null_mut()).unwrap();

        if count == 0 {
            return Err(DeviceError::SecondaryEmpty);
        }

        self.size_array = count as usize;
        // warn!("Read burst from ring: {}", self.size_array);
        Ok(self.size_array)
    }

    // We currently prioritize packets from the MMDS over regular network packets.
    fn read_from_mmds_or_tap(&mut self) -> Result<usize> {
        //Removed by Mihai - MMDS stuff removed.
        // if let Some(ns) = self.mmds_ns.as_mut() {
        //     if let Some(len) =
        //         ns.write_next_frame(frame_bytes_from_buf_mut(&mut self.rx_frame_buf)?)
        //     {
        //         let len = len.get();
        //         METRICS.mmds.tx_frames.inc();
        //         METRICS.mmds.tx_bytes.add(len);
        //         init_vnet_hdr(&mut self.rx_frame_buf);
        //         return Ok(vnet_hdr_len() + len);
        //     }
        // }
    
        self.read_tap().map_err(Error::IO)
    }

    fn process_rx(&mut self) -> result::Result<(), DeviceError> {
        // Read as many frames as possible.
        loop {
            match self.read_from_secondary_burst() {
                Ok(count) => {
                    self.rx_bytes_read = count;
                    METRICS.net.rx_count.inc();
                    while self.size_array != 0 {
                        if !self.rate_limited_rx_single_frame() {
                            // Info by Mihai
                            // Gets here only if write to guest fails with EmptyQueue or AddUsed
                            self.rx_deferred_frame = true;
                            break;
                        }
                    }
                    // if it is not deferred, then all array was written on guest memory
                    // and it will loop again to read a burst
                    if self.rx_deferred_frame == true {
                        break;
                    }
                }
                // If the channel was empty then break
                Err(DeviceError::SecondaryEmpty) => {
                    break;
                }
                Err(_) => {
                    warn!("This should not happen and I have to chance the error type of the read_from_secondary_burst func");
                }
            }
        }
        // At this point we processed as many Rx frames as possible.
        // We have to wake the guest if at least one descriptor chain has been used.
        self.signal_rx_used_queue()
    }

    // Process the deferred frame first, then continue reading from tap.
    fn handle_deferred_frame(&mut self) -> result::Result<(), DeviceError> {
        if self.rate_limited_rx_single_frame() {
            self.rx_deferred_frame = false;
            // process_rx() was interrupted possibly before consuming all
            // packets in the tap; try continuing now.
            return self.process_rx();
        }

        self.signal_rx_used_queue()
    }

    fn resume_rx(&mut self) -> result::Result<(), DeviceError> {
        // Info added by Mihai
        // The only reason I could have a deferred frame after removing rate limiter is because of
        // Empty Queue error or AddUsed error.
        if self.rx_deferred_frame {
            self.handle_deferred_frame()
        } else {
            Ok(())
        }
    }

    fn process_tx(&mut self) -> result::Result<(), DeviceError> {
        let mem = match self.device_state {
            DeviceState::Activated(ref mem) => mem,
            // This should never happen, it's been already validated in the event handler.
            DeviceState::Inactive => unreachable!(),
        };

        // The MMDS network stack works like a state machine, based on synchronous calls, and
        // without being added to any event loop. If any frame is accepted by the MMDS, we also
        // trigger a process_rx() which checks if there are any new frames to be sent, starting
        // with the MMDS network stack.
        let mut process_rx_for_mmds = false;
        let mut raise_irq = false;
        let tx_queue = &mut self.queues[TX_INDEX];

        // Plan: creez un array de lungime fixa in care imi pun mbuf-urile
        // Citesc din guest -> pun in mbuf -> pun in array
        // Cand se unmple array, dau enqueue_burst (cat timp dai enqueue burst?)
        // Apoi continui iar sa citesc din guest si repet procesul
        const burst_size: usize = 512;
        let mut array_mbufs = [null_mut(); burst_size];
        let mut index_array = 0;

        while let Some(head) = tx_queue.pop(mem) {

            let now: Instant = Instant::now();
            let time_spent: Duration = now.duration_since(self.last_tx);
            if time_spent.as_secs() > 10 {
                warn!("10s: {}", self.total_tx);
                self.total_tx = 0;
                self.last_tx = Instant::now();
            }

            let head_index = head.index;
            let mut read_count = 0;
            let mut next_desc = Some(head);
            
            let mut was_nothing_to_read = 0;

            self.tx_iovec.clear();
            while let Some(desc) = next_desc {
                if desc.is_write_only() {
                    self.tx_iovec.clear();
                    was_nothing_to_read = 1;
                    break;
                }
                self.tx_iovec.push((desc.addr, desc.len as usize));
                read_count += desc.len as usize;
                next_desc = desc.next_descriptor();
            }

            // Would pointlessly get an mbuf if there is nothing to read.
            if was_nothing_to_read == 1 {
                tx_queue
                    .add_used(mem, head_index, 0)
                    .map_err(DeviceError::QueueError)?;
                // I should not raise irq from here in case there was nothing to read.
                // The raise irq is not only after reading at least something. And it
                // will be done in case something is actually read.

                // raise_irq = true;
                warn!("TX: write only case");
                continue;
            }

            // Get an mbuf from mempool
            let mut mbuf_fresh = self.client.do_rte_mempool_get();
            while let Err(_er) = mbuf_fresh {
                // Fails if not enough entries are available.
                warn!("rte_mempool_get failed, trying again!");
                mbuf_fresh = self.client.do_rte_mempool_get();
            }
            // It should never be an error now.
            let my_mbuf = mbuf_fresh.unwrap();
            let mbuf_struct: *mut rte_mbuf = my_mbuf as *mut rte_mbuf;

            // I will put the packet from guest memory inside this slice
            let mbuf_data = unsafe {
                let buf_addr: *mut u8 = (*mbuf_struct).buf_addr as *mut u8;

                // I get a reference to the begining of data inside the mbuf and return it
                let real_buf_addr = buf_addr.offset(((*mbuf_struct).data_off) as isize);
                // size of packet - headroom
                let mbuf_data_slice = std::slice::from_raw_parts_mut(real_buf_addr, 65500 - 128);
                mbuf_data_slice
            };


            read_count = 0;
            // Copy buffer from across multiple descriptors.
            // TODO(performance - Issue #420): change this to use `writev()` instead of `write()`
            // and get rid of the intermediate buffer.
            for (desc_addr, desc_len) in self.tx_iovec.drain(..) {
                let limit = cmp::min((read_count + desc_len) as usize, mbuf_data.len());

                let read_result = mem.read_slice(
                    &mut mbuf_data[read_count..limit as usize],
                    desc_addr,
                );
                match read_result {
                    Ok(()) => {
                        read_count += limit - read_count;
                        METRICS.net.tx_count.inc();
                    }
                    Err(e) => {
                        error!("Failed to read slice: {:?}", e);
                        match e {
                            GuestMemoryError::PartialBuffer { .. } => &METRICS.net.tx_partial_reads,
                            _ => &METRICS.net.tx_fails,
                        }
                        .inc();
                        read_count = 0;
                        break;
                    }
                }
            }

            // Set necessary fields of mbuf
            unsafe {
                // Get rid of vnet header by increasing the offset
                // This offset points at the beginning of data.
                (*mbuf_struct).data_off = (*mbuf_struct).data_off + vnet_hdr_len() as u16;
                (*mbuf_struct).data_len = (read_count - vnet_hdr_len() as usize) as u16;
                (*mbuf_struct).pkt_len = (read_count - vnet_hdr_len() as usize) as u32;
                (*mbuf_struct).nb_segs = 1;
                (*mbuf_struct).ol_flags = PKT_TX_TCP_CKSUM;
                // warn!("{}", (*mbuf_struct).data_len);
            }

            if burst_size > index_array + 1 {
                array_mbufs[index_array] = my_mbuf;
                index_array = index_array + 1;
            } else {
                // Am umplut array-ul cu mbuf-uri si trebuie sa-l trimit.
                 // Now I have to enqueue the mbuf
                self.client.enqueue_burst_untill_done(array_mbufs.as_mut_ptr(), burst_size as u32, null_mut());
                // Count the number of mbufs successfully enqed
                self.total_tx = self.total_tx + index_array;
                index_array = 0;
                // warn!("Enq: {}", burst_size);
            }

            tx_queue
                .add_used(mem, head_index, 0)
                .map_err(DeviceError::QueueError)?;
            raise_irq = true;
        }
    
        if index_array != 0 {
            let nr_mbufs: u32 = index_array as u32;
            self.client.enqueue_burst_untill_done(array_mbufs.as_mut_ptr(), nr_mbufs, null_mut());
            // count the number of mbufs successfully enqed
            self.total_tx = self.total_tx + index_array;
            // warn!("Enq: {}", nr_mbufs);
        }
    
        if raise_irq {
            self.signal_used_queue()?;
        } else {
            METRICS.net.no_tx_avail_buffer.inc();
        }

        Ok(())
    }

    /// Updates the parameters for the rate limiters
    pub fn patch_rate_limiters(
        &mut self,
        rx_bytes: BucketUpdate,
        rx_ops: BucketUpdate,
        tx_bytes: BucketUpdate,
        tx_ops: BucketUpdate,
    ) {
        self.rx_rate_limiter.update_buckets(rx_bytes, rx_ops);
        self.tx_rate_limiter.update_buckets(tx_bytes, tx_ops);
    }

    #[cfg(not(test))]
    fn read_tap(&mut self) -> io::Result<usize> {
        self.tap.read(&mut self.rx_frame_buf)
    }

    pub fn process_rx_queue_event(&mut self) {
        METRICS.net.rx_queue_event_count.inc();

        if let Err(e) = self.queue_evts[RX_INDEX].read() {
            // rate limiters present but with _very high_ allowed rate
            error!("Failed to get rx queue event: {:?}", e);
            METRICS.net.event_fails.inc();
        } else {
            // Added by Mihai - removed the rate limiter check.
            self.resume_rx().unwrap_or_else(report_net_event_fail);
        }
    }

    pub fn process_tap_rx_event(&mut self) {
        let mem = match self.device_state {
            DeviceState::Activated(ref mem) => mem,
            // This should never happen, it's been already validated in the event handler.
            DeviceState::Inactive => unreachable!(),
        };
        METRICS.net.rx_tap_event_count.inc();

        // While there are no available RX queue buffers and there's a deferred_frame
        // don't process any more incoming. Otherwise start processing a frame. In the
        // process the deferred_frame flag will be set in order to avoid freezing the
        // RX queue.
        if self.queues[RX_INDEX].is_empty(mem) && self.rx_deferred_frame {
            METRICS.net.no_rx_avail_buffer.inc();
            return;
        }

        if self.rx_deferred_frame
        // Process a deferred frame first if available. Don't read from tap again
        // until we manage to receive this deferred frame.
        {   
            warn!("HANDLE DEFFERED FRAME");
            self.handle_deferred_frame()
                .unwrap_or_else(report_net_event_fail);
        } else {
            self.process_rx().unwrap_or_else(report_net_event_fail);
        }
    }

    pub fn process_tx_queue_event(&mut self) {
        

        METRICS.net.tx_queue_event_count.inc();
        if let Err(e) = self.queue_evts[TX_INDEX].read() {
            error!("Failed to get tx queue event: {:?}", e);
            METRICS.net.event_fails.inc();
        } else if !self.tx_rate_limiter.is_blocked()
        // If the limiter is not blocked, continue transmitting bytes.
        {
            self.process_tx().unwrap_or_else(report_net_event_fail);
        } else {
            METRICS.net.tx_rate_limiter_throttled.inc();
        }
    }

    pub fn process_rx_rate_limiter_event(&mut self) {
        METRICS.net.rx_event_rate_limiter_count.inc();
        // Upon rate limiter event, call the rate limiter handler
        // and restart processing the queue.

        match self.rx_rate_limiter.event_handler() {
            Ok(_) => {
                // There might be enough budget now to receive the frame.
                self.resume_rx().unwrap_or_else(report_net_event_fail);
            }
            Err(e) => {
                error!("Failed to get rx rate-limiter event: {:?}", e);
                METRICS.net.event_fails.inc();
            }
        }
    }

    pub fn process_tx_rate_limiter_event(&mut self) {
        METRICS.net.tx_rate_limiter_event_count.inc();
        // Upon rate limiter event, call the rate limiter handler
        // and restart processing the queue.
        match self.tx_rate_limiter.event_handler() {
            Ok(_) => {
                // There might be enough budget now to send the frame.
                self.process_tx().unwrap_or_else(report_net_event_fail);
            }
            Err(e) => {
                error!("Failed to get tx rate-limiter event: {:?}", e);
                METRICS.net.event_fails.inc();
            }
        }
    }

    /// Process device virtio queue(s).
    pub fn process_virtio_queues(&mut self) {
        let _ = self.resume_rx();
        let _ = self.process_tx();
    }
}

impl VirtioDevice for Net {
    fn device_type(&self) -> u32 {
        TYPE_NET
    }

    fn queues(&self) -> &[Queue] {
        &self.queues
    }

    fn queues_mut(&mut self) -> &mut [Queue] {
        &mut self.queues
    }

    fn queue_events(&self) -> &[EventFd] {
        &self.queue_evts
    }

    fn interrupt_evt(&self) -> &EventFd {
        &self.interrupt_evt
    }

    fn interrupt_status(&self) -> Arc<AtomicUsize> {
        self.interrupt_status.clone()
    }

    fn avail_features(&self) -> u64 {
        self.avail_features
    }

    fn acked_features(&self) -> u64 {
        self.acked_features
    }

    fn set_acked_features(&mut self, acked_features: u64) {
        self.acked_features = acked_features;
    }

    fn read_config(&self, offset: u64, mut data: &mut [u8]) {
        let config_space_bytes = self.config_space.as_slice();
        let config_len = config_space_bytes.len() as u64;
        if offset >= config_len {
            error!("Failed to read config space");
            METRICS.net.cfg_fails.inc();
            return;
        }
        if let Some(end) = offset.checked_add(data.len() as u64) {
            // This write can't fail, offset and end are checked against config_len.
            data.write_all(
                &config_space_bytes[offset as usize..cmp::min(end, config_len) as usize],
            )
            .unwrap();
        }
    }

    fn write_config(&mut self, offset: u64, data: &[u8]) {
        let data_len = data.len() as u64;
        let config_space_bytes = self.config_space.as_mut_slice();
        let config_len = config_space_bytes.len() as u64;
        if offset + data_len > config_len {
            error!("Failed to write config space");
            METRICS.net.cfg_fails.inc();
            return;
        }

        config_space_bytes[offset as usize..(offset + data_len) as usize].copy_from_slice(data);
        self.guest_mac = Some(MacAddr::from_bytes_unchecked(
            &self.config_space.guest_mac[..MAC_ADDR_LEN],
        ));
        METRICS.net.mac_address_updates.inc();
    }

    fn is_activated(&self) -> bool {
        match self.device_state {
            DeviceState::Inactive => false,
            DeviceState::Activated(_) => true,
        }
    }

    fn activate(&mut self, mem: GuestMemoryMmap) -> ActivateResult {
        if self.activate_evt.write(1).is_err() {
            error!("Net: Cannot write to activate_evt");
            return Err(super::super::ActivateError::BadActivate);
        }
        self.device_state = DeviceState::Activated(mem);
        Ok(())
    }
}

#[cfg(test)]
#[macro_use]
pub mod tests {
    use super::*;
    use crate::virtio::net::device::{
        frame_bytes_from_buf, frame_bytes_from_buf_mut, init_vnet_hdr, vnet_hdr_len,
    };
    use std::net::Ipv4Addr;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use std::{io, mem, thread};

    use crate::check_metric_after_block;
    use crate::virtio::net::test_utils::test::TestHelper;
    use crate::virtio::net::test_utils::{
        check_used_queue_signal, default_net, if_index, inject_tap_tx_frame, set_mac, NetEvent,
        NetQueue, ReadTapMock, TapTrafficSimulator,
    };
    use crate::virtio::net::QUEUE_SIZES;
    use crate::virtio::{
        Net, VirtioDevice, MAX_BUFFER_SIZE, RX_INDEX, TX_INDEX, TYPE_NET, VIRTIO_MMIO_INT_VRING,
        VIRTQ_DESC_F_WRITE,
    };
    use dumbo::pdu::arp::{EthIPv4ArpFrame, ETH_IPV4_FRAME_LEN};
    use dumbo::pdu::ethernet::ETHERTYPE_ARP;
    use logger::{IncMetric, METRICS};
    use rate_limiter::{RateLimiter, TokenBucket, TokenType};
    use virtio_gen::virtio_net::{
        virtio_net_hdr_v1, VIRTIO_F_VERSION_1, VIRTIO_NET_F_CSUM, VIRTIO_NET_F_GUEST_CSUM,
        VIRTIO_NET_F_GUEST_TSO4, VIRTIO_NET_F_GUEST_UFO, VIRTIO_NET_F_HOST_TSO4,
        VIRTIO_NET_F_HOST_UFO, VIRTIO_NET_F_MAC,
    };
    use vm_memory::{Address, GuestMemory};

    impl Net {
        pub fn read_tap(&mut self) -> io::Result<usize> {
            match &self.mocks.read_tap {
                ReadTapMock::MockFrame(frame) => {
                    self.rx_frame_buf[..frame.len()].copy_from_slice(&frame);
                    Ok(frame.len())
                }
                ReadTapMock::Failure => Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Read tap synthetically failed.",
                )),
                ReadTapMock::TapFrame => self.tap.read(&mut self.rx_frame_buf),
            }
        }
    }

    #[test]
    fn test_vnet_helpers() {
        let mut frame_buf = vec![42u8; vnet_hdr_len() - 1];
        assert_eq!(
            format!("{:?}", frame_bytes_from_buf(&frame_buf)),
            "Err(VnetHeaderMissing)"
        );
        assert_eq!(
            format!("{:?}", frame_bytes_from_buf_mut(&mut frame_buf)),
            "Err(VnetHeaderMissing)"
        );

        let mut frame_buf: [u8; MAX_BUFFER_SIZE] = [42u8; MAX_BUFFER_SIZE];

        let vnet_hdr_len_ = mem::size_of::<virtio_net_hdr_v1>();
        assert_eq!(vnet_hdr_len_, vnet_hdr_len());

        init_vnet_hdr(&mut frame_buf);
        let zero_vnet_hdr = vec![0u8; vnet_hdr_len_];
        assert_eq!(zero_vnet_hdr, &frame_buf[..vnet_hdr_len_]);

        let payload = vec![42u8; MAX_BUFFER_SIZE - vnet_hdr_len_];
        assert_eq!(payload, frame_bytes_from_buf(&frame_buf).unwrap());

        {
            let payload = frame_bytes_from_buf_mut(&mut frame_buf).unwrap();
            payload[0] = 15;
        }
        assert_eq!(frame_buf[vnet_hdr_len_], 15);
    }

    #[test]
    fn test_virtio_device_type() {
        let mut net = default_net();
        set_mac(&mut net, MacAddr::parse_str("11:22:33:44:55:66").unwrap());
        assert_eq!(net.device_type(), TYPE_NET);
    }

    #[test]
    fn test_virtio_device_features() {
        let mut net = default_net();
        set_mac(&mut net, MacAddr::parse_str("11:22:33:44:55:66").unwrap());

        // Test `features()` and `ack_features()`.
        let features = 1 << VIRTIO_NET_F_GUEST_CSUM
            | 1 << VIRTIO_NET_F_CSUM
            | 1 << VIRTIO_NET_F_GUEST_TSO4
            | 1 << VIRTIO_NET_F_MAC
            | 1 << VIRTIO_NET_F_GUEST_UFO
            | 1 << VIRTIO_NET_F_HOST_TSO4
            | 1 << VIRTIO_NET_F_HOST_UFO
            | 1 << VIRTIO_F_VERSION_1;

        assert_eq!(net.avail_features_by_page(0), features as u32);
        assert_eq!(net.avail_features_by_page(1), (features >> 32) as u32);
        for i in 2..10 {
            assert_eq!(net.avail_features_by_page(i), 0u32);
        }

        for i in 0..10 {
            net.ack_features_by_page(i, std::u32::MAX);
        }

        assert_eq!(net.acked_features, features);
    }

    #[test]
    fn test_virtio_device_read_config() {
        let mut net = default_net();
        set_mac(&mut net, MacAddr::parse_str("11:22:33:44:55:66").unwrap());

        // Test `read_config()`. This also validates the MAC was properly configured.
        let mac = MacAddr::parse_str("11:22:33:44:55:66").unwrap();
        let mut config_mac = [0u8; MAC_ADDR_LEN];
        net.read_config(0, &mut config_mac);
        assert_eq!(config_mac, mac.get_bytes());

        // Invalid read.
        config_mac = [0u8; MAC_ADDR_LEN];
        net.read_config(MAC_ADDR_LEN as u64 + 1, &mut config_mac);
        assert_eq!(config_mac, [0u8, 0u8, 0u8, 0u8, 0u8, 0u8]);
    }

    #[test]
    fn test_virtio_device_rewrite_config() {
        let mut net = default_net();
        set_mac(&mut net, MacAddr::parse_str("11:22:33:44:55:66").unwrap());

        let new_config: [u8; 6] = [0x66, 0x55, 0x44, 0x33, 0x22, 0x11];
        net.write_config(0, &new_config);
        let mut new_config_read = [0u8; 6];
        net.read_config(0, &mut new_config_read);
        assert_eq!(new_config, new_config_read);

        // Check that the guest MAC was updated.
        let expected_guest_mac = MacAddr::from_bytes_unchecked(&new_config);
        assert_eq!(expected_guest_mac, net.guest_mac.unwrap());
        assert_eq!(METRICS.net.mac_address_updates.count(), 1);

        // Partial write (this is how the kernel sets a new mac address) - byte by byte.
        let new_config = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        for i in 0..new_config.len() {
            net.write_config(i as u64, &new_config[i..=i]);
        }
        net.read_config(0, &mut new_config_read);
        assert_eq!(new_config, new_config_read);

        // Invalid write.
        net.write_config(5, &new_config);
        // Verify old config was untouched.
        new_config_read = [0u8; 6];
        net.read_config(0, &mut new_config_read);
        assert_eq!(new_config, new_config_read);
    }

    #[test]
    fn test_rx_missing_queue_signal() {
        let mut th = TestHelper::default();
        th.activate_net();

        th.add_desc_chain(NetQueue::Rx, 0, &[(0, 4096, VIRTQ_DESC_F_WRITE)]);
        th.net().queue_evts[RX_INDEX].read().unwrap();
        check_metric_after_block!(
            METRICS.net.event_fails,
            1,
            th.simulate_event(NetEvent::RxQueue)
        );

        // Check that the used queue didn't advance.
        assert_eq!(th.rxq.used.idx.get(), 0);
    }

    #[test]
    fn test_rx_read_only_descriptor() {
        let mut th = TestHelper::default();
        th.activate_net();

        th.add_desc_chain(
            NetQueue::Rx,
            0,
            &[
                (0, 100, VIRTQ_DESC_F_WRITE),
                (1, 100, 0),
                (2, 1000, VIRTQ_DESC_F_WRITE),
            ],
        );
        let frame = th.check_rx_deferred_frame(1000);
        th.rxq.check_used_elem(0, 0, 0);

        th.check_rx_queue_resume(&frame);
    }

    #[test]
    fn test_rx_short_writable_descriptor() {
        let mut th = TestHelper::default();
        th.activate_net();

        th.add_desc_chain(NetQueue::Rx, 0, &[(0, 100, VIRTQ_DESC_F_WRITE)]);
        let frame = th.check_rx_deferred_frame(1000);
        th.rxq.check_used_elem(0, 0, 0);

        th.check_rx_queue_resume(&frame);
    }

    #[test]
    fn test_rx_partial_write() {
        let mut th = TestHelper::default();
        th.activate_net();

        // The descriptor chain is created so that the last descriptor doesn't fit in the
        // guest memory.
        let offset = th.mem.last_addr().raw_value() - th.data_addr() - 300;
        th.add_desc_chain(
            NetQueue::Rx,
            offset,
            &[
                (0, 100, VIRTQ_DESC_F_WRITE),
                (1, 50, VIRTQ_DESC_F_WRITE),
                (2, 4096, VIRTQ_DESC_F_WRITE),
            ],
        );
        let frame = th.check_rx_deferred_frame(1000);
        th.rxq.check_used_elem(0, 0, 0);

        th.check_rx_queue_resume(&frame);
    }

    #[test]
    fn test_rx_retry() {
        let mut th = TestHelper::default();
        th.activate_net();
        th.net().mocks.set_read_tap(ReadTapMock::TapFrame);

        // Add invalid descriptor chain - read only descriptor.
        th.add_desc_chain(
            NetQueue::Rx,
            0,
            &[
                (0, 100, VIRTQ_DESC_F_WRITE),
                (1, 100, 0),
                (2, 1000, VIRTQ_DESC_F_WRITE),
            ],
        );
        // Add invalid descriptor chain - too short.
        th.add_desc_chain(NetQueue::Rx, 1200, &[(3, 100, VIRTQ_DESC_F_WRITE)]);
        // Add invalid descriptor chain - invalid memory offset.
        th.add_desc_chain(
            NetQueue::Rx,
            th.mem.last_addr().raw_value(),
            &[(4, 1000, VIRTQ_DESC_F_WRITE)],
        );

        // Add valid descriptor chain.
        th.add_desc_chain(NetQueue::Rx, 1300, &[(5, 1000, VIRTQ_DESC_F_WRITE)]);

        // Inject frame to tap and run epoll.
        let frame = inject_tap_tx_frame(&th.net(), 1000);
        check_metric_after_block!(
            METRICS.net.rx_packets_count,
            1,
            th.event_manager.run_with_timeout(100).unwrap()
        );

        // Check that the used queue has advanced.
        assert_eq!(th.rxq.used.idx.get(), 4);
        check_used_queue_signal(&th.net(), 1);
        // Check that the invalid descriptor chains have been discarded
        th.rxq.check_used_elem(0, 0, 0);
        th.rxq.check_used_elem(1, 3, 0);
        th.rxq.check_used_elem(2, 4, 0);
        // Check that the frame wasn't deferred.
        assert!(!th.net().rx_deferred_frame);
        // Check that the frame has been written successfully to the valid Rx descriptor chain.
        th.rxq.check_used_elem(3, 5, frame.len() as u32);
        th.rxq.dtable[5].check_data(&frame);
    }

    #[test]
    fn test_rx_complex_desc_chain() {
        let mut th = TestHelper::default();
        th.activate_net();
        th.net().mocks.set_read_tap(ReadTapMock::TapFrame);

        // Create a valid Rx avail descriptor chain with multiple descriptors.
        th.add_desc_chain(
            NetQueue::Rx,
            0,
            // Add gaps between the descriptor ids in order to ensure that we follow
            // the `next` field.
            &[
                (3, 100, VIRTQ_DESC_F_WRITE),
                (5, 50, VIRTQ_DESC_F_WRITE),
                (11, 4096, VIRTQ_DESC_F_WRITE),
            ],
        );
        // Inject frame to tap and run epoll.
        let frame = inject_tap_tx_frame(&th.net(), 1000);
        check_metric_after_block!(
            METRICS.net.rx_packets_count,
            1,
            th.event_manager.run_with_timeout(100).unwrap()
        );

        // Check that the frame wasn't deferred.
        assert!(!th.net().rx_deferred_frame);
        // Check that the used queue has advanced.
        assert_eq!(th.rxq.used.idx.get(), 1);
        check_used_queue_signal(&th.net(), 1);
        // Check that the frame has been written successfully to the Rx descriptor chain.
        th.rxq.check_used_elem(0, 3, frame.len() as u32);
        th.rxq.dtable[3].check_data(&frame[..100]);
        th.rxq.dtable[5].check_data(&frame[100..150]);
        th.rxq.dtable[11].check_data(&frame[150..]);
    }

    #[test]
    fn test_rx_multiple_frames() {
        let mut th = TestHelper::default();
        th.activate_net();
        th.net().mocks.set_read_tap(ReadTapMock::TapFrame);

        // Create 2 valid Rx avail descriptor chains. Each one has enough space to fit the
        // following 2 frames. But only 1 frame has to be written to each chain.
        th.add_desc_chain(
            NetQueue::Rx,
            0,
            &[(0, 500, VIRTQ_DESC_F_WRITE), (1, 500, VIRTQ_DESC_F_WRITE)],
        );
        th.add_desc_chain(
            NetQueue::Rx,
            1000,
            &[(2, 500, VIRTQ_DESC_F_WRITE), (3, 500, VIRTQ_DESC_F_WRITE)],
        );
        // Inject 2 frames to tap and run epoll.
        let frame_1 = inject_tap_tx_frame(&th.net(), 200);
        let frame_2 = inject_tap_tx_frame(&th.net(), 300);
        check_metric_after_block!(
            METRICS.net.rx_packets_count,
            2,
            th.event_manager.run_with_timeout(100).unwrap()
        );

        // Check that the frames weren't deferred.
        assert!(!th.net().rx_deferred_frame);
        // Check that the used queue has advanced.
        assert_eq!(th.rxq.used.idx.get(), 2);
        check_used_queue_signal(&th.net(), 1);
        // Check that the 1st frame was written successfully to the 1st Rx descriptor chain.
        th.rxq.check_used_elem(0, 0, frame_1.len() as u32);
        th.rxq.dtable[0].check_data(&frame_1);
        th.rxq.dtable[1].check_data(&[0; 500]);
        // Check that the 2nd frame was written successfully to the 2nd Rx descriptor chain.
        th.rxq.check_used_elem(1, 2, frame_2.len() as u32);
        th.rxq.dtable[2].check_data(&frame_2);
        th.rxq.dtable[3].check_data(&[0; 500]);
    }

    #[test]
    fn test_tx_missing_queue_signal() {
        let mut th = TestHelper::default();
        th.activate_net();
        let tap_traffic_simulator = TapTrafficSimulator::new(if_index(&th.net().tap));

        th.add_desc_chain(NetQueue::Tx, 0, &[(0, 4096, 0)]);
        th.net().queue_evts[TX_INDEX].read().unwrap();
        check_metric_after_block!(
            METRICS.net.event_fails,
            1,
            th.simulate_event(NetEvent::TxQueue)
        );

        // Check that the used queue didn't advance.
        assert_eq!(th.txq.used.idx.get(), 0);
        // Check that the frame wasn't sent to the tap.
        assert!(!tap_traffic_simulator.pop_rx_packet(&mut [0; 1000]));
    }

    #[test]
    fn test_tx_writeable_descriptor() {
        let mut th = TestHelper::default();
        th.activate_net();
        let tap_traffic_simulator = TapTrafficSimulator::new(if_index(&th.net().tap));

        let desc_list = [(0, 100, 0), (1, 100, VIRTQ_DESC_F_WRITE), (2, 500, 0)];
        th.add_desc_chain(NetQueue::Tx, 0, &desc_list);
        th.write_tx_frame(&desc_list, 700);
        th.event_manager.run_with_timeout(100).unwrap();

        // Check that the used queue advanced.
        assert_eq!(th.txq.used.idx.get(), 1);
        check_used_queue_signal(&th.net(), 1);
        th.txq.check_used_elem(0, 0, 0);
        // Check that the frame was skipped.
        assert!(!tap_traffic_simulator.pop_rx_packet(&mut []));
    }

    #[test]
    fn test_tx_short_frame() {
        let mut th = TestHelper::default();
        th.activate_net();
        let tap_traffic_simulator = TapTrafficSimulator::new(if_index(&th.net().tap));

        // Send an invalid frame (too small, VNET header missing).
        th.add_desc_chain(NetQueue::Tx, 0, &[(0, 1, 0)]);
        check_metric_after_block!(
            &METRICS.net.tx_malformed_frames,
            1,
            th.event_manager.run_with_timeout(100)
        );

        // Check that the used queue advanced.
        assert_eq!(th.txq.used.idx.get(), 1);
        check_used_queue_signal(&th.net(), 1);
        th.txq.check_used_elem(0, 0, 0);
        // Check that the frame was skipped.
        assert!(!tap_traffic_simulator.pop_rx_packet(&mut []));
    }

    #[test]
    fn test_tx_partial_read() {
        let mut th = TestHelper::default();
        th.activate_net();
        let tap_traffic_simulator = TapTrafficSimulator::new(if_index(&th.net().tap));

        // The descriptor chain is created so that the last descriptor doesn't fit in the
        // guest memory.
        let offset = th.mem.last_addr().raw_value() + 1 - th.data_addr() - 300;
        let desc_list = [(0, 100, 0), (1, 50, 0), (2, 4096, 0)];
        th.add_desc_chain(NetQueue::Tx, offset, &desc_list);
        let expected_len =
            (150 + th.mem.last_addr().raw_value() + 1 - th.txq.dtable[2].addr.get()) as usize;
        th.write_tx_frame(&desc_list, expected_len);
        check_metric_after_block!(
            METRICS.net.tx_partial_reads,
            1,
            th.event_manager.run_with_timeout(100).unwrap()
        );

        // Check that the used queue advanced.
        assert_eq!(th.txq.used.idx.get(), 1);
        check_used_queue_signal(&th.net(), 1);
        th.txq.check_used_elem(0, 0, 0);
        // Check that the frame was skipped.
        assert!(!tap_traffic_simulator.pop_rx_packet(&mut []));
    }

    #[test]
    fn test_tx_retry() {
        let mut th = TestHelper::default();
        th.activate_net();
        let tap_traffic_simulator = TapTrafficSimulator::new(if_index(&th.net().tap));

        // Add invalid descriptor chain - writeable descriptor.
        th.add_desc_chain(
            NetQueue::Tx,
            0,
            &[(0, 100, 0), (1, 100, VIRTQ_DESC_F_WRITE), (2, 500, 0)],
        );
        // Add invalid descriptor chain - invalid memory.
        th.add_desc_chain(NetQueue::Tx, th.mem.last_addr().raw_value(), &[(3, 100, 0)]);
        // Add invalid descriptor chain - too short.
        th.add_desc_chain(NetQueue::Tx, 700, &[(0, 1, 0)]);

        // Add valid descriptor chain
        let desc_list = [(4, 1000, 0)];
        th.add_desc_chain(NetQueue::Tx, 0, &desc_list);
        let frame = th.write_tx_frame(&desc_list, 1000);

        check_metric_after_block!(
            &METRICS.net.tx_malformed_frames,
            3,
            th.event_manager.run_with_timeout(100)
        );

        // Check that the used queue advanced.
        assert_eq!(th.txq.used.idx.get(), 4);
        check_used_queue_signal(&th.net(), 1);
        th.txq.check_used_elem(3, 4, 0);
        // Check that the valid frame was sent to the tap.
        let mut buf = vec![0; 1000];
        assert!(tap_traffic_simulator.pop_rx_packet(&mut buf[vnet_hdr_len()..]));
        assert_eq!(&buf, &frame);
        // Check that no other frame was sent to the tap.
        assert!(!tap_traffic_simulator.pop_rx_packet(&mut []));
    }

    #[test]
    fn test_tx_complex_descriptor() {
        let mut th = TestHelper::default();
        th.activate_net();
        let tap_traffic_simulator = TapTrafficSimulator::new(if_index(&th.net().tap));

        // Add gaps between the descriptor ids in order to ensure that we follow
        // the `next` field.
        let desc_list = [(3, 100, 0), (5, 50, 0), (11, 850, 0)];
        th.add_desc_chain(NetQueue::Tx, 0, &desc_list);
        let frame = th.write_tx_frame(&desc_list, 1000);

        check_metric_after_block!(
            METRICS.net.tx_packets_count,
            1,
            th.event_manager.run_with_timeout(100).unwrap()
        );

        // Check that the used queue advanced.
        assert_eq!(th.txq.used.idx.get(), 1);
        check_used_queue_signal(&th.net(), 1);
        th.txq.check_used_elem(0, 3, 0);
        // Check that the frame was sent to the tap.
        let mut buf = vec![0; 1000];
        assert!(tap_traffic_simulator.pop_rx_packet(&mut buf[vnet_hdr_len()..]));
        assert_eq!(&buf[..1000], &frame[..1000]);
    }

    #[test]
    fn test_tx_multiple_frame() {
        let mut th = TestHelper::default();
        th.activate_net();
        let tap_traffic_simulator = TapTrafficSimulator::new(if_index(&th.net().tap));

        // Write the first frame to the Tx queue
        let desc_list = [(0, 50, 0), (1, 100, 0), (2, 150, 0)];
        th.add_desc_chain(NetQueue::Tx, 0, &desc_list);
        let frame_1 = th.write_tx_frame(&desc_list, 300);
        // Write the second frame to the Tx queue
        let desc_list = [(3, 100, 0), (4, 200, 0), (5, 300, 0)];
        th.add_desc_chain(NetQueue::Tx, 500, &desc_list);
        let frame_2 = th.write_tx_frame(&desc_list, 600);

        check_metric_after_block!(
            METRICS.net.tx_packets_count,
            2,
            th.event_manager.run_with_timeout(100).unwrap()
        );

        // Check that the used queue advanced.
        assert_eq!(th.txq.used.idx.get(), 2);
        check_used_queue_signal(&th.net(), 1);
        th.txq.check_used_elem(0, 0, 0);
        th.txq.check_used_elem(1, 3, 0);
        // Check that the first frame was sent to the tap.
        let mut buf = vec![0; 300];
        assert!(tap_traffic_simulator.pop_rx_packet(&mut buf[vnet_hdr_len()..]));
        assert_eq!(&buf[..300], &frame_1[..300]);
        // Check that the second frame was sent to the tap.
        let mut buf = vec![0; 600];
        assert!(tap_traffic_simulator.pop_rx_packet(&mut buf[vnet_hdr_len()..]));
        assert_eq!(&buf[..600], &frame_2[..600]);
    }

    fn create_arp_request(
        src_mac: MacAddr,
        src_ip: Ipv4Addr,
        dst_mac: MacAddr,
        dst_ip: Ipv4Addr,
    ) -> ([u8; MAX_BUFFER_SIZE], usize) {
        let mut frame_buf = [b'\0'; MAX_BUFFER_SIZE];
        let frame_len;
        // Create an ethernet frame.
        let incomplete_frame = EthernetFrame::write_incomplete(
            frame_bytes_from_buf_mut(&mut frame_buf).unwrap(),
            dst_mac,
            src_mac,
            ETHERTYPE_ARP,
        )
        .ok()
        .unwrap();
        // Set its length to hold an ARP request.
        let mut frame = incomplete_frame.with_payload_len_unchecked(ETH_IPV4_FRAME_LEN);

        // Save the total frame length.
        frame_len = vnet_hdr_len() + frame.payload_offset() + ETH_IPV4_FRAME_LEN;

        // Create the ARP request.
        let arp_request =
            EthIPv4ArpFrame::write_request(frame.payload_mut(), src_mac, src_ip, dst_mac, dst_ip);
        // Validate success.
        assert!(arp_request.is_ok());

        (frame_buf, frame_len)
    }

    #[test]
    fn test_mmds_detour_and_injection() {
        let mut net = default_net();

        let src_mac = MacAddr::parse_str("11:11:11:11:11:11").unwrap();
        let src_ip = Ipv4Addr::new(10, 1, 2, 3);
        let dst_mac = MacAddr::parse_str("22:22:22:22:22:22").unwrap();
        let dst_ip = Ipv4Addr::new(169, 254, 169, 254);

        let (frame_buf, frame_len) = create_arp_request(src_mac, src_ip, dst_mac, dst_ip);

        // Call the code which sends the packet to the host or MMDS.
        // Validate the frame was consumed by MMDS and that the metrics reflect that.
        check_metric_after_block!(
            &METRICS.mmds.rx_accepted,
            1,
            assert!(Net::write_to_mmds_or_tap(
                net.mmds_ns.as_mut(),
                &mut net.tx_rate_limiter,
                &frame_buf[..frame_len],
                &mut net.tap,
                Some(src_mac),
            )
            .unwrap())
        );

        // Validate that MMDS has a response and we can retrieve it.
        check_metric_after_block!(
            &METRICS.mmds.tx_frames,
            1,
            net.read_from_mmds_or_tap().unwrap()
        );
    }

    #[test]
    fn test_mac_spoofing_detection() {
        let mut net = default_net();

        let guest_mac = MacAddr::parse_str("11:11:11:11:11:11").unwrap();
        let not_guest_mac = MacAddr::parse_str("33:33:33:33:33:33").unwrap();
        let guest_ip = Ipv4Addr::new(10, 1, 2, 3);
        let dst_mac = MacAddr::parse_str("22:22:22:22:22:22").unwrap();
        let dst_ip = Ipv4Addr::new(10, 1, 1, 1);

        let (frame_buf, frame_len) = create_arp_request(guest_mac, guest_ip, dst_mac, dst_ip);

        // Check that a legit MAC doesn't affect the spoofed MAC metric.
        check_metric_after_block!(
            &METRICS.net.tx_spoofed_mac_count,
            0,
            Net::write_to_mmds_or_tap(
                net.mmds_ns.as_mut(),
                &mut net.tx_rate_limiter,
                &frame_buf[..frame_len],
                &mut net.tap,
                Some(guest_mac),
            )
        );

        // Check that a spoofed MAC increases our spoofed MAC metric.
        check_metric_after_block!(
            &METRICS.net.tx_spoofed_mac_count,
            1,
            Net::write_to_mmds_or_tap(
                net.mmds_ns.as_mut(),
                &mut net.tx_rate_limiter,
                &frame_buf[..frame_len],
                &mut net.tap,
                Some(not_guest_mac),
            )
        );
    }

    #[test]
    fn test_process_error_cases() {
        let mut th = TestHelper::default();
        th.activate_net();

        // RX rate limiter events should error since the limiter is not blocked.
        // Validate that the event failed and failure was properly accounted for.
        check_metric_after_block!(
            &METRICS.net.event_fails,
            1,
            th.simulate_event(NetEvent::RxRateLimiter)
        );

        // TX rate limiter events should error since the limiter is not blocked.
        // Validate that the event failed and failure was properly accounted for.
        check_metric_after_block!(
            &METRICS.net.event_fails,
            1,
            th.simulate_event(NetEvent::TxRateLimiter)
        );
    }

    // Cannot easily test failures for:
    //  * queue_evt.read (rx and tx)
    //  * interrupt_evt.write
    #[test]
    fn test_read_tap_fail_event_handler() {
        let mut th = TestHelper::default();
        th.activate_net();
        th.net().mocks.set_read_tap(ReadTapMock::Failure);

        // The RX queue is empty and rx_deffered_frame is set.
        th.net().rx_deferred_frame = true;
        check_metric_after_block!(
            &METRICS.net.no_rx_avail_buffer,
            1,
            th.simulate_event(NetEvent::Tap)
        );

        // Fake an avail buffer; this time, tap reading should error out.
        th.rxq.avail.idx.set(1);
        check_metric_after_block!(
            &METRICS.net.tap_read_fails,
            1,
            th.simulate_event(NetEvent::Tap)
        );
    }

    #[test]
    fn test_rx_rate_limiter_handling() {
        let mut th = TestHelper::default();
        th.activate_net();

        th.net().rx_rate_limiter = RateLimiter::new(0, 0, 0, 0, 0, 0).unwrap();
        // There is no actual event on the rate limiter's timerfd.
        check_metric_after_block!(
            &METRICS.net.event_fails,
            1,
            th.simulate_event(NetEvent::RxRateLimiter)
        );
    }

    #[test]
    fn test_tx_rate_limiter_handling() {
        let mut th = TestHelper::default();
        th.activate_net();

        th.net().tx_rate_limiter = RateLimiter::new(0, 0, 0, 0, 0, 0).unwrap();
        th.simulate_event(NetEvent::TxRateLimiter);
        // There is no actual event on the rate limiter's timerfd.
        check_metric_after_block!(
            &METRICS.net.event_fails,
            1,
            th.simulate_event(NetEvent::TxRateLimiter)
        );
    }

    #[test]
    fn test_bandwidth_rate_limiter() {
        let mut th = TestHelper::default();
        th.activate_net();

        // Test TX bandwidth rate limiting
        {
            // create bandwidth rate limiter that allows 40960 bytes/s with bucket size 4096 bytes
            let mut rl = RateLimiter::new(0x1000, 0, 100, 0, 0, 0).unwrap();
            // use up the budget
            assert!(rl.consume(0x1000, TokenType::Bytes));

            // set this tx rate limiter to be used
            th.net().tx_rate_limiter = rl;

            // try doing TX
            // following TX procedure should fail because of bandwidth rate limiting
            {
                // trigger the TX handler
                th.add_desc_chain(NetQueue::Tx, 0, &[(0, 4096, 0)]);
                th.simulate_event(NetEvent::TxQueue);

                // assert that limiter is blocked
                assert!(th.net().tx_rate_limiter.is_blocked());
                assert_eq!(METRICS.net.tx_rate_limiter_throttled.count(), 1);
                // make sure the data is still queued for processing
                assert_eq!(th.txq.used.idx.get(), 0);
            }

            // wait for 100ms to give the rate-limiter timer a chance to replenish
            // wait for an extra 100ms to make sure the timerfd event makes its way from the kernel
            thread::sleep(Duration::from_millis(200));

            // following TX procedure should succeed because bandwidth should now be available
            {
                // tx_count increments 1 from process_tx() and 1 from write_to_mmds_or_tap()
                check_metric_after_block!(
                    &METRICS.net.tx_count,
                    2,
                    th.simulate_event(NetEvent::TxRateLimiter)
                );
                // validate the rate_limiter is no longer blocked
                assert!(!th.net().tx_rate_limiter.is_blocked());
                // make sure the data queue advanced
                assert_eq!(th.txq.used.idx.get(), 1);
            }
        }

        // Test RX bandwidth rate limiting
        {
            // create bandwidth rate limiter that allows 40960 bytes/s with bucket size 4096 bytes
            let mut rl = RateLimiter::new(0x1000, 0, 100, 0, 0, 0).unwrap();
            // use up the budget
            assert!(rl.consume(0x1000, TokenType::Bytes));

            // set this rx rate limiter to be used
            th.net().rx_rate_limiter = rl;

            // set up RX
            assert!(!th.net().rx_deferred_frame);
            th.add_desc_chain(NetQueue::Rx, 0, &[(0, 4096, VIRTQ_DESC_F_WRITE)]);

            // following RX procedure should fail because of bandwidth rate limiting
            {
                // trigger the RX handler
                th.simulate_event(NetEvent::Tap);

                // assert that limiter is blocked
                assert!(th.net().rx_rate_limiter.is_blocked());
                assert_eq!(METRICS.net.rx_rate_limiter_throttled.count(), 1);
                assert!(th.net().rx_deferred_frame);
                // assert that no operation actually completed (limiter blocked it)
                check_used_queue_signal(&th.net(), 1);
                // make sure the data is still queued for processing
                assert_eq!(th.rxq.used.idx.get(), 0);
            }

            // wait for 100ms to give the rate-limiter timer a chance to replenish
            // wait for an extra 100ms to make sure the timerfd event makes its way from the kernel
            thread::sleep(Duration::from_millis(200));

            // following RX procedure should succeed because bandwidth should now be available
            {
                let frame = &th.net().mocks.read_tap.mock_frame();
                // no longer throttled
                check_metric_after_block!(
                    &METRICS.net.rx_rate_limiter_throttled,
                    0,
                    th.simulate_event(NetEvent::RxRateLimiter)
                );
                // validate the rate_limiter is no longer blocked
                assert!(!th.net().rx_rate_limiter.is_blocked());
                // make sure the virtio queue operation completed this time
                check_used_queue_signal(&th.net(), 1);
                // make sure the data queue advanced
                assert_eq!(th.rxq.used.idx.get(), 1);
                th.rxq.check_used_elem(0, 0, frame.len() as u32);
                th.rxq.dtable[0].check_data(&frame);
            }
        }
    }

    #[test]
    fn test_ops_rate_limiter() {
        let mut th = TestHelper::default();
        th.activate_net();

        // Test TX ops rate limiting
        {
            // create ops rate limiter that allows 10 ops/s with bucket size 1 ops
            let mut rl = RateLimiter::new(0, 0, 0, 1, 0, 100).unwrap();
            // use up the budget
            assert!(rl.consume(1, TokenType::Ops));

            // set this tx rate limiter to be used
            th.net().tx_rate_limiter = rl;

            // try doing TX
            // following TX procedure should fail because of ops rate limiting
            {
                // trigger the TX handler
                th.add_desc_chain(NetQueue::Tx, 0, &[(0, 4096, 0)]);
                check_metric_after_block!(
                    METRICS.net.tx_rate_limiter_throttled,
                    1,
                    th.simulate_event(NetEvent::TxQueue)
                );

                // assert that limiter is blocked
                assert!(th.net().tx_rate_limiter.is_blocked());
                // make sure the data is still queued for processing
                assert_eq!(th.txq.used.idx.get(), 0);
            }

            // wait for 100ms to give the rate-limiter timer a chance to replenish
            // wait for an extra 100ms to make sure the timerfd event makes its way from the kernel
            thread::sleep(Duration::from_millis(200));

            // following TX procedure should succeed because ops should now be available
            {
                // no longer throttled
                check_metric_after_block!(
                    &METRICS.net.tx_rate_limiter_throttled,
                    0,
                    th.simulate_event(NetEvent::TxRateLimiter)
                );
                // validate the rate_limiter is no longer blocked
                assert!(!th.net().tx_rate_limiter.is_blocked());
                // make sure the data queue advanced
                assert_eq!(th.txq.used.idx.get(), 1);
            }
        }

        // Test RX ops rate limiting
        {
            // create ops rate limiter that allows 10 ops/s with bucket size 1 ops
            let mut rl = RateLimiter::new(0, 0, 0, 1, 0, 100).unwrap();
            // use up the initial budget
            assert!(rl.consume(1, TokenType::Ops));

            // set this rx rate limiter to be used
            th.net().rx_rate_limiter = rl;

            // set up RX
            assert!(!th.net().rx_deferred_frame);
            th.add_desc_chain(NetQueue::Rx, 0, &[(0, 4096, VIRTQ_DESC_F_WRITE)]);

            // following RX procedure should fail because of ops rate limiting
            {
                // trigger the RX handler
                check_metric_after_block!(
                    METRICS.net.rx_rate_limiter_throttled,
                    1,
                    th.simulate_event(NetEvent::Tap)
                );

                // assert that limiter is blocked
                assert!(th.net().rx_rate_limiter.is_blocked());
                assert!(METRICS.net.rx_rate_limiter_throttled.count() >= 1);
                assert!(th.net().rx_deferred_frame);
                // assert that no operation actually completed (limiter blocked it)
                check_used_queue_signal(&th.net(), 1);
                // make sure the data is still queued for processing
                assert_eq!(th.rxq.used.idx.get(), 0);

                // trigger the RX handler again, this time it should do the limiter fast path exit
                th.simulate_event(NetEvent::Tap);
                // assert that no operation actually completed, that the limiter blocked it
                check_used_queue_signal(&th.net(), 0);
                // make sure the data is still queued for processing
                assert_eq!(th.rxq.used.idx.get(), 0);
            }

            // wait for 100ms to give the rate-limiter timer a chance to replenish
            // wait for an extra 100ms to make sure the timerfd event makes its way from the kernel
            thread::sleep(Duration::from_millis(200));

            // following RX procedure should succeed because ops should now be available
            {
                let frame = &th.net().mocks.read_tap.mock_frame();
                th.simulate_event(NetEvent::RxRateLimiter);
                // make sure the virtio queue operation completed this time
                check_used_queue_signal(&th.net(), 1);
                // make sure the data queue advanced
                assert_eq!(th.rxq.used.idx.get(), 1);
                th.rxq.check_used_elem(0, 0, frame.len() as u32);
                th.rxq.dtable[0].check_data(&frame);
            }
        }
    }

    #[test]
    fn test_patch_rate_limiters() {
        let mut th = TestHelper::default();
        th.activate_net();

        th.net().rx_rate_limiter = RateLimiter::new(10, 0, 10, 2, 0, 2).unwrap();
        th.net().tx_rate_limiter = RateLimiter::new(10, 0, 10, 2, 0, 2).unwrap();

        let rx_bytes = TokenBucket::new(1000, 1001, 1002).unwrap();
        let rx_ops = TokenBucket::new(1003, 1004, 1005).unwrap();
        let tx_bytes = TokenBucket::new(1006, 1007, 1008).unwrap();
        let tx_ops = TokenBucket::new(1009, 1010, 1011).unwrap();

        th.net().patch_rate_limiters(
            BucketUpdate::Update(rx_bytes.clone()),
            BucketUpdate::Update(rx_ops.clone()),
            BucketUpdate::Update(tx_bytes.clone()),
            BucketUpdate::Update(tx_ops.clone()),
        );
        let compare_buckets = |a: &TokenBucket, b: &TokenBucket| {
            assert_eq!(a.capacity(), b.capacity());
            assert_eq!(a.one_time_burst(), b.one_time_burst());
            assert_eq!(a.refill_time_ms(), b.refill_time_ms());
        };
        compare_buckets(th.net().rx_rate_limiter.bandwidth().unwrap(), &rx_bytes);
        compare_buckets(th.net().rx_rate_limiter.ops().unwrap(), &rx_ops);
        compare_buckets(th.net().tx_rate_limiter.bandwidth().unwrap(), &tx_bytes);
        compare_buckets(th.net().tx_rate_limiter.ops().unwrap(), &tx_ops);

        th.net().patch_rate_limiters(
            BucketUpdate::Disabled,
            BucketUpdate::Disabled,
            BucketUpdate::Disabled,
            BucketUpdate::Disabled,
        );
        assert!(th.net().rx_rate_limiter.bandwidth().is_none());
        assert!(th.net().rx_rate_limiter.ops().is_none());
        assert!(th.net().tx_rate_limiter.bandwidth().is_none());
        assert!(th.net().tx_rate_limiter.ops().is_none());
    }

    #[test]
    fn test_virtio_device() {
        let mut th = TestHelper::default();
        th.activate_net();
        let net = th.net.lock().unwrap();

        // Test queues count (TX and RX).
        let queues = net.queues();
        assert_eq!(queues.len(), QUEUE_SIZES.len());
        assert_eq!(queues[RX_INDEX].size, th.rxq.size());
        assert_eq!(queues[TX_INDEX].size, th.txq.size());

        // Test corresponding queues events.
        assert_eq!(net.queue_events().len(), QUEUE_SIZES.len());

        // Test interrupts.
        let interrupt_status = net.interrupt_status();
        interrupt_status.fetch_or(VIRTIO_MMIO_INT_VRING as usize, Ordering::SeqCst);
        assert_eq!(
            interrupt_status.load(Ordering::SeqCst),
            VIRTIO_MMIO_INT_VRING as usize
        );

        check_used_queue_signal(&net, 0);
    }
}
