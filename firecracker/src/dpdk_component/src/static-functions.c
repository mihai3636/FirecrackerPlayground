#include "../include/static-functions.h"

int rte_ring_dequeue_real(struct rte_ring *r, void **obj_p)
{
    return rte_ring_dequeue(r, obj_p);
}

int rte_ring_enqueue_real(struct rte_ring *r, void *obj)
{
    return rte_ring_enqueue(r, obj);
}

int rte_mempool_get_real(struct rte_mempool *mp, void **obj_p) {
    return rte_mempool_get(mp, obj_p);
}

void rte_mempool_put_real(struct rte_mempool *mp, void *obj) {
    rte_mempool_put(mp, obj);
}

struct rte_mbuf *rte_pktmbuf_alloc_real(struct rte_mempool *mp) {
    rte_pktmbuf_alloc(mp);
}
