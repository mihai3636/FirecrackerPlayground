#include <rte_ring.h>
#include <rte_mempool.h>
#include <rte_mbuf.h>

int rte_ring_dequeue_real(struct rte_ring *r, void **obj_p);
