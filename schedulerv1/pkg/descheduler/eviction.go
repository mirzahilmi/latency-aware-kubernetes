package descheduler

import (
	"context"
	"strings"

	corev1 "k8s.io/api/core/v1"
	policyv1 "k8s.io/api/policy/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"github.com/rs/zerolog/log"
)

func (d *AdaptiveDescheduler) evictIdlePod(ctx context.Context, nodeName string) (bool, error) {
	target, err := d.getMostIdlePod(ctx, nodeName)
	if err != nil {
		return false, err
	}
	if target == nil {
		log.Warn().Msgf("[EVICTION] No idle pod found to evict on node %s", nodeName)
		return false, nil
	}

	grace := int64(30)

	eviction := &policyv1.Eviction{
		ObjectMeta: metav1.ObjectMeta{
			Name:      target.Name,
			Namespace: target.Namespace,
			UID:       target.UID,
		},
		DeleteOptions: &metav1.DeleteOptions{
			GracePeriodSeconds: &grace,
			Preconditions: &metav1.Preconditions{
				UID: &target.UID,
			},
		},
	}

	log.Info().Msgf("[EVICTION] Evicting pod %s/%s", target.Namespace, target.Name)

	if err := d.kubeClient.PolicyV1().Evictions(target.Namespace).Evict(ctx, eviction); err != nil {
		return false, err
	}

	return true, nil
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
