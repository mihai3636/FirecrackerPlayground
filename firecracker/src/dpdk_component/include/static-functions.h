#include <rte_ring.h>
#include <rte_mempool.h>
#include <rte_mbuf.h>

int rte_ring_dequeue_real(struct rte_ring *r, void **obj_p);
int rte_ring_enqueue_real(struct rte_ring *r, void *obj);
int rte_mempool_get_real(struct rte_mempool *mp, void **obj_p);
void rte_mempool_put_real(struct rte_mempool *mp, void *obj);
int rte_ring_empty_real(struct rte_ring *r);
unsigned int rte_ring_enqueue_burst_real(struct rte_ring *r, void* const* obj_table, unsigned int n, unsigned int* free_space);
unsigned int rte_ring_dequeue_burst_real(struct rte_ring *r, void **obj_table, unsigned int n, unsigned int* available);
char* rte_pktmbuf_prepend_real(struct rte_mbuf *m, uint16_t len);