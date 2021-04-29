Readme by Mihai

On this branch I will add the logic to send packets from Secondary DPDK to Guest VM inside Firecracker.


Found out why there are two interrupts for RX.

One interrupt is on the tap_fd descriptor.
This one triggers every time data arrives on the TAP interface.
After this interrupt is triggered, the net devices begins
reading a frame from tap, writing to guest, then again the same steps.
It does it until there is no more to read, or until it is stopped by the
rate limiter.
If it is stopped by the rate limiter, the flag
self.rx_deferred_frame
is set to true and the whole reading from tap stops.

UPDATE:

WARNING:

DO NOT get rid of deferred frame completely!
a deferred frame could be because of the rate limiter
OR because there are not enough RX queues available.

Second interrupt is on the self.queue_evts[RX_INDEX].as_raw_fd()
descriptor.
This one is triggered by the Guest I assume and it does so in order
to check for deferred frame.
From what I noticed, this interrupt is triggered after every
read cycle started by event on tap_fd.
So the Guest is checking "just to be sure" that there is nothing left there.

I am not planning on using the rate limiting mechanism so it shouldn't be an issue to me.

Idee:

Fac un nou EventFd pe care il adaug in interest_list.
Dau trigger pe event_fd-ul asta din DPDK Secondary atunci cand primesc pachet (s-ar putea sa dau prea multe triggere aici, WARNING)

Mai fac un channel prin care trimit date de la Secondary catre Net Device.

In event_handler.rs:
Schimb tap_fd cu noul event_fd
Merg pe logica la receive cand se triggeruieste tap_fd si mai schimb in functie de necesitati.

CHANGELOG:

Solved the TCP Handshake issue.
The suspicion from last udpate was true.
I made DPDK primary to offload TCP checksum to hardware, modified the code in primary dpdk.
There was no need to calculate the pseudo header checksum in primary because the packet
came with the pseudoheader calculated from guest.

iperf3 -s on microVM then iperf3 -c 10.2 on physical host is working.

Found another issue: iperf3 is not working when microVM is acting as a sender.
Investigation going on.
Suspicion: the data copied from guest memory is not a single packet. It's too much data there
and I assumed it was a single packet.

Check bug_log.txt
--------------------------------------------------------------------------------------
Found an issue: TCP Handshakes are not working.
The packets sent from microVM are not computing the right TCP Checksum.
Need to investigate: check TcpChecksumInvestigation.txt

From the above investigation I believe the following:

the virtio headers flag is set to VIRTIO_NET_F_CSUM whcih means that the
checksum calculation is assumed to be done by the device.

The checksum indicated by the packet which just got out from the guest memory
is exactly the number indicated by wireshark, which means DPDK did not alted the checksum.

According the virtio documentation, the value of the checksum is equal to the sum of the
TCP pseudoheader. (the value computed by the microVM)

---------------------------------------------------------------------------------------
Packets are flowing both in and out of the Guest using the DPDK interface now.
Solved the GSO error by adding a vnet header.
Used the existing init functions for vnet header, the bytes are all set to 0.

-------------------------------------------------------------------------------------------
Still not solve GSO error.
search for gso in firecracker repository
found something in virtio_gen -> src -> virtio_net.rs
Also check the tests in tap.rs, I think you should put a vnet header before the packet.

PING 10.0.0.1 (10.0.0.1): 56 data bytes
[   36.922204] eth0: bad gso: type: 252, size: 256
[   37.942333] eth0: bad gso: type: 252, size: 256
[   38.954425] eth0: bad gso: type: 252, size: 256

---------------------------------------------------------------

Added logic to read from secondary instead of reading from tap.
Getting error when using ping: bad gso, type: 252, size: 256

--------------------------------------------------------------------
Removed the rate limiting logic for receive packet interrupts.
Commented it out. ("Removed by Mihai")
Additional info at ("Info by Mihai")
-------------------------------------------------------
WARNING:

DO NOT get rid of deferred frame completely!
a deferred frame could be because of the rate limiter
OR because there are not enough RX queues available.

check pub fn process_tap_rx_event(&mut self)
-------------------------------------------------------------------
in event_handler.rs

Added the event_dpdk_secondary to the interest_list
Added the event_dpdk_secondary to the process "match". Calls a dummy function

in device.rs
Added a dummy handler for event_dpdk_secondary to see if the event works.

in dpdk_component::client.rs
Triggering the event_dpdk_secondary when packet can be sent from secondary to Net Device

-----------------------------------------------------------------------
in client.rs

Added sender_channel and event_dpdk_secondary
as constructor param and inside the ClientDpdk struct def.

in device.rs

Created a new channel and passed sending end to DpdkClient,
Created a new eventFd and passed it to DpdkClient
Compiling working.
