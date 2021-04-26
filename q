[33mcommit 8ef583b28e2f6e28219a6cf47c17e3a1ad652d8a[m[33m ([m[1;36mHEAD -> [m[1;32mdpdk_component[m[33m, [m[1;31morigin/dpdk_component[m[33m)[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Mon Apr 19 19:23:10 2021 +0300

    Managed to put L2 packet in primary DPDK. Is it ok for internet?

[33mcommit 9601755a9fd1a62fa6628de06a7e06c92b4b2dd6[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Mon Apr 19 17:44:20 2021 +0300

    Printing info about mbuf, trying to place packet in mbuf

[33mcommit 076259e518a85613413d6b21a64fa3fc421b9e38[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Fri Apr 16 22:12:07 2021 +0300

    Sending packet from Fc thread to Dpdk thread

[33mcommit 170b07b5defd403383a8db775747f12c49d84eac[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Fri Apr 16 18:01:58 2021 +0300

    Updated readme

[33mcommit f2904fa66c7b1cfadbbbc3561d457d0d35e5e332[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Fri Apr 16 17:57:13 2021 +0300

    mempool_get, ring_enqueue tested. Sending empty buf to primary tested.

[33mcommit ac219ea6f2c32690f4980688d59a03ff55e0bdfb[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 15 21:05:25 2021 +0300

    Wrapper for rte_mempool_put binding, everything compiling.

[33mcommit f3dc280c75629d34e7cc35c25b05b3a42193abe8[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 15 20:59:47 2021 +0300

    Wrote a c binding for rte_mempool_put

[33mcommit fde3f70770e481d16fd7156b4ec4baafe9ee8b41[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 15 20:45:53 2021 +0300

    rte_ring_enqueue wrapper compiling, not tested

[33mcommit 6bee5698121f2ca7883851a39cc3b46d4f73ec5e[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 15 16:56:19 2021 +0300

    rte_mempool_get wrapper compiling, not tested.

[33mcommit 2da8244043e3039bb895b267c5c4af225c059b94[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 15 16:45:20 2021 +0300

    Added binding signature to rte_mempool_get

[33mcommit 10172948477b0340416fc43110c0c94ac4764aa9[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 15 15:34:49 2021 +0300

    Added binding signatures rte_ring_enqueue()

[33mcommit 1fe098240e19cab3bc94e91d688a9411cf7eea3c[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 15 13:17:58 2021 +0300

    rte_mempool_lookup working

[33mcommit 5831ac7693cd83d64c4e2785765408f3e282f2c2[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 15 12:42:52 2021 +0300

    Attach rings working.

[33mcommit afa7910c1175159031658c48b0ad6119d5ad8174[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Wed Apr 14 22:02:40 2021 +0300

    Rings and pool init code, not compiling yet

[33mcommit bf2407a7373c32b511e8de36e5d47bfe781c360e[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Wed Apr 14 20:14:55 2021 +0300

    rte_eal_init working, connecting ssh as root

[33mcommit 21ce204fff49a86287b3363260ffec691fd8b975[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Mon Apr 12 21:41:36 2021 +0300

    Before using bindingsErrno

[33mcommit cff8f3f5409fb05a81a8e91a134b4764609e4b76[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Sun Apr 11 20:03:33 2021 +0300

    init eal working but only if sudo firecracker

[33mcommit b0619890e63c2e517305508cfa1b5c13a1649698[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Sun Apr 11 12:43:33 2021 +0300

    Managed to use dpdk_component crate inside Net struct

[33mcommit ec4f94e82196356ad54565f0686b3fd746daeeac[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Sun Apr 11 11:30:44 2021 +0300

    New build command for faster time.

[33mcommit 66e168101a6c30859df4c367c77bee5fef02e64e[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Sat Apr 10 19:26:38 2021 +0300

    Fixed the wrongfully usage of modules. Now it takes too long to build the whole firecracker.

[33mcommit 455e57175fecb103ade1cd473a3cf834e5fa6b9e[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Sat Apr 10 17:52:46 2021 +0300

    Found a build command to overcome missing headers. Check Readme. Now modules are buggy

[33mcommit 1f78e5d3f6d41034ab46765f97d3143460445518[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Sat Apr 10 14:14:03 2021 +0300

    Tried to include some more files, still not working.

[33mcommit 691d148529d713b006f14a672f1621efffa54548[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Fri Apr 9 16:15:16 2021 +0300

    Trying to build the create, not working.

[33mcommit 8b6203e9082fda13cae637c309eedf5d6f587bdc[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Fri Apr 9 13:02:04 2021 +0300

    Updated readme

[33mcommit f4737affd1a3b7274ebe106551256f4d0e2f93d9[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Fri Apr 9 12:56:03 2021 +0300

    Trying to add the dpdk component crate

[33mcommit 7cf25efcd22f952a13329ea65332973d3243caa8[m[33m ([m[1;31morigin/main[m[33m, [m[1;32mmain[m[33m)[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Thu Apr 8 20:11:40 2021 +0300

    Added a dummy thread attempt.

[33mcommit 8a9babe980e5f4c41972e8b08e62c48f24f678d0[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Wed Apr 7 15:49:13 2021 +0300

    script for starting guest

[33mcommit e9650409c4347e0c836dac17c01c3f97eb61b975[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Wed Apr 7 15:48:05 2021 +0300

    Log file

[33mcommit e4f1f4887dcb07a3899549080ec0d5b95f548889[m
Author: mihaidogaru2537 <dogarumihai2537@gmail.com>
Date:   Sun Mar 28 13:54:56 2021 +0300

    first commit
