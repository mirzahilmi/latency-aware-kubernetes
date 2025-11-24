package descheduler

import (
	"context"
	"strings"

	corev1 "k8s.io/api/core/v1"
	policyv1 "k8s.io/api/policy/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"github.com/rs/zerolog/log"
)

func (d *AdaptiveDescheduler) evictIdlePod(ctx context.Context, nodeName string) error {
	target, err := d.getMostIdlePod(ctx, nodeName)
	if err != nil {
		return err
	}
	if target == nil {
		log.Warn().Msgf("[EVICTION] No idle pod found to evict on node %s", nodeName)
		return nil
	}

	eviction := &policyv1.Eviction{
		ObjectMeta: metav1.ObjectMeta{
			Name:      target.Name,
			Namespace: target.Namespace,
		},
		DeleteOptions: &metav1.DeleteOptions{},
	}

	log.Info().Msgf("[EVICTION] Evicting pod %s/%s (CPU idle) from %s", target.Namespace, target.Name, nodeName)
	return d.kubeClient.PolicyV1().Evictions(target.Namespace).Evict(ctx, eviction)
}

// skip system/infra pods
func isSystemPod(p corev1.Pod) bool {
	if strings.Contains(p.Name, "scheduler") ||
		strings.Contains(p.Name, "descheduler") ||
		strings.Contains(p.Name, "coredns") ||
		strings.Contains(p.Name, "metrics") {
		return true
	}
	for _, ref := range p.OwnerReferences {
		if ref.Kind == "DaemonSet" {
			return true
		}
	}
	return false
}
