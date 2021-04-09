#include "../include/static-functions.h"

int rte_ring_dequeue_real(struct rte_ring *r, void **obj_p)
{
    return rte_ring_dequeue(r, obj_p);
}