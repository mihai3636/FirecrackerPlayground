Readme by Mihai

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

Second interrupt is on the self.queue_evts[RX_INDEX].as_raw_fd()
descriptor.
This one is triggered by the Guest I assume and it does so in order
to check for deferred frame.
From what I noticed, this interrupt is triggered after every
read cycle started by event on tap_fd.
So the Guest is checking "just to be sure" that there is nothing left there.

I am not planning on using the rate limiting mechanism so it shouldn't be an issue to me.
