package descheduler

import (
	"context"
	"math"

	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"github.com/rs/zerolog/log"
)

// find target pod to evicts
func (d *AdaptiveDescheduler) getMostIdlePod(ctx context.Context, nodeName string) (*corev1.Pod, error) {
	pods, err := d.clientset.CoreV1().Pods(d.namespace).List(ctx, metav1.ListOptions{
		FieldSelector: "spec.nodeName=" + nodeName,
	})
	if err != nil {
		return nil, err
	}
	if len(pods.Items) == 0 {
		log.Debug().Msgf("[DESCHEDULER] No pods found on %s", nodeName)
		return nil, nil
	}

	podMetrics, err := d.metricsClient.MetricsV1beta1().PodMetricses(d.namespace).List(ctx, metav1.ListOptions{
		FieldSelector: "metadata.namespace=" + d.namespace,
	})
	if err != nil {
		return nil, err
	}

	minCPU := math.MaxFloat64
	var target *corev1.Pod

	for _, pod := range pods.Items {
		if isSystemPod(pod) {
			continue
		}
		var totalCPU float64
		for _, m := range podMetrics.Items {
			if m.Name == pod.Name {
				for _, c := range m.Containers {
					q := c.Usage[corev1.ResourceCPU]
					totalCPU += float64(q.MilliValue())
				}
			}
		}
		if totalCPU < minCPU && totalCPU < d.policy.IdleCPUThreshold {
			minCPU = totalCPU
			target = &pod
		}
	}

	// fallback: if all pods have 0 usage, evict one non-system pod anyway
	if target == nil && minCPU == math.MaxFloat64 {
		log.Warn().Msgf("[DESCHEDULER] Found some pods on %s that have 0 CPU usage, evicting first non-system pod", nodeName)
		for _, pod := range pods.Items {
			if !isSystemPod(pod) {
				target = &pod
				break
			}
		}
	}

	if target != nil {
		log.Info().Msgf("[DESCHEDULER] Selected idle pod candidate: %s/%s (CPU=%.2fm, threshold=%.2fm)",
			target.Namespace, target.Name, minCPU, d.policy.IdleCPUThreshold)
	} else {
		log.Info().Msgf("[DESCHEDULER] No idle pod available to evict on %s (threshold=%.2fm)",
			nodeName, d.policy.IdleCPUThreshold)
	}

	return target, nil
}

