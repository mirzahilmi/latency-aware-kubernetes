package descheduler

import (
	"context"
	"math/rand"
	"os"
	"strings"

	"github.com/rs/zerolog/log"
	corev1 "k8s.io/api/core/v1"
	policyv1 "k8s.io/api/policy/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// evictPodsForRebalancing mengevict satu pod acak dari prevTopNode (node yang sebelumnya jadi top)
func (d *Descheduler) evictPodsForRebalancing(ctx context.Context, prevTopNode string) {
	log.Info().Msgf("[DESCHEDULER] Triggering eviction from previous top node: %s", prevTopNode)

	ns := os.Getenv("POD_NAMESPACE")

	// ambil semua pod di node sebelumnya
	pods, err := d.clientset.CoreV1().Pods(ns).List(ctx,
		metav1.ListOptions{FieldSelector: "spec.nodeName=" + prevTopNode})
	if err != nil {
		log.Warn().Err(err).Msgf("[DESCHEDULER] Failed to list pods on node %s", prevTopNode)
		return
	}
	if len(pods.Items) == 0 {
		log.Warn().Msgf("[DESCHEDULER] No pods found on node %s", prevTopNode)
		return
	}

	// filter: hanya non-system pods
	candidates := make([]corev1.Pod, 0)
	for _, p := range pods.Items {
		if !isSystemPod(p) {
			candidates = append(candidates, p)
		}
	}

	if len(candidates) == 0 {
		log.Warn().Msgf("[DESCHEDULER] No non-system pods found on node %s", prevTopNode)
		return
	}

	// pilih satu pod acak dari prevTopNode
	target := candidates[rand.Intn(len(candidates))]
	log.Warn().Msgf("[DESCHEDULER] Evicting pod %s from previous top node %s", target.Name, prevTopNode)

	eviction := &policyv1.Eviction{
		ObjectMeta: metav1.ObjectMeta{
			Name:      target.Name,
			Namespace: target.Namespace,
		},
	}

	if err := d.clientset.PolicyV1().Evictions(target.Namespace).Evict(ctx, eviction); err != nil {
		log.Warn().Err(err).Msgf("[DESCHEDULER] Eviction failed for pod %s", target.Name)
		return
	}

	log.Info().Msgf("[DESCHEDULER] Successfully evicted pod %s from node %s", target.Name, prevTopNode)
}

// helper buat skip pod sistem (scheduler, descheduler, dsb)
func isSystemPod(p corev1.Pod) bool {
	if strings.Contains(p.Name, "scheduler") ||
		strings.Contains(p.Name, "descheduler") ||
		strings.Contains(p.Name, "metrics") ||
		strings.Contains(p.Name, "coredns") {
		return true
	}
	for _, ref := range p.OwnerReferences {
		if ref.Kind == "DaemonSet" {
			return true
		}
	}
	return false
}
