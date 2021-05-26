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

int rte_ring_empty_real(struct rte_ring *r) {
    return rte_ring_empty(r);
}

unsigned int rte_ring_enqueue_burst_real(struct rte_ring *r, void* const* obj_table, unsigned int n, unsigned int* free_space) {
    return rte_ring_enqueue_burst(r, obj_table, n, free_space);
}

unsigned int rte_ring_dequeue_burst_real(struct rte_ring *r, void **obj_table, unsigned int n, unsigned int* available) {
    return rte_ring_dequeue_burst(r, obj_table, n, available);
}

char* rte_pktmbuf_prepend_real(struct rte_mbuf *m, uint16_t len) {
    return rte_pktmbuf_prepend(m, len);
}