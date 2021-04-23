#include <rte_ring.h>
#include <rte_mempool.h>
#include <rte_mbuf.h>

int rte_ring_dequeue_real(struct rte_ring *r, void **obj_p);
int rte_ring_enqueue_real(struct rte_ring *r, void *obj);
int rte_mempool_get_real(struct rte_mempool *mp, void **obj_p);
void rte_mempool_put_real(struct rte_mempool *mp, void *obj);
struct rte_mbuf *rte_pktmbuf_alloc_real(struct rte_mempool *mp);
