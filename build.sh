#!/bin/bash

cd ./firecracker
RUSTFLAGS="-lrte_eal -lrte_mempool -lrte_mbuf -lrte_ring -C target-feature=-crt-static" cargo build --target=x86_64-unknown-linux-gnu

