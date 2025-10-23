package descheduler

import (
	"context"
	"os"

	"github.com/rs/zerolog/log"
	policyv1 "k8s.io/api/policy/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// evictPodsForRebalancing mengevict satu pod idle dari node yang sebelumnya jadi top
func (d *Descheduler) evictPodsForRebalancing(ctx context.Context, node string) {
	log.Info().Msgf("[DESCHEDULER] Evicting 1 idle pod from previous top node: %s", node)

	// Cari pod idle di node itu
	idlePodName, minCPU := d.findIdlePod(ctx, node)
	if idlePodName == "" {
		log.Info().Msgf("No idle pod found on node %s", node)
		return
	}

	log.Warn().Msgf("[DESCHEDULER] Evicting idle pod %s (CPU %dm) from node %s",
		idlePodName, minCPU, node)

	ns := os.Getenv("POD_NAMESPACE")

	eviction := &policyv1.Eviction{
		ObjectMeta: metav1.ObjectMeta{
			Name:      idlePodName,
			Namespace: ns,
		},
	}

	if err := d.clientset.PolicyV1().Evictions(ns).Evict(ctx, eviction); err != nil {
		log.Warn().Err(err).Msgf("Eviction failed for %s", idlePodName)
		return
	}

	log.Info().Msgf("✅ Evicted idle pod %s from previous top node %s", idlePodName, node)
}
