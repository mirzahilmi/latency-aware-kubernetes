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

	podMetrics, err := d.metricsClient.MetricsV1beta1().PodMetricses(d.namespace).List(ctx, metav1.ListOptions{})
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
		validMetric := false
	
		for _, m := range podMetrics.Items {
			if m.Name == pod.Name {
				for _, c := range m.Containers {
					q := c.Usage[corev1.ResourceCPU]
					v := float64(q.MilliValue())
					if v >= 0 && !math.IsNaN(v) && !math.IsInf(v, 0) {
						totalCPU += v
						validMetric = true
					}
				}
			}
		}

		if !validMetric {
			log.Warn().Msgf("[DESCHEDULER] No valid CPU metric for pod %s/%s, skipping", pod.Namespace, pod.Name)
			continue
		}

		if totalCPU < minCPU && totalCPU < float64(d.policy.IdleCPUThreshold) {
			log.Debug().Msgf("[DESCHEDULER] Pod %s/%s has %.2fm CPU usage (< threshold %.dm)", 
				pod.Namespace, pod.Name, totalCPU, d.policy.IdleCPUThreshold)
			minCPU = totalCPU
			target = &pod
		}
	}

	if target != nil {
		log.Info().Msgf("[DESCHEDULER] Selected idle pod candidate: %s/%s (CPU=%.2fm, threshold=%.2fm)",
			target.Namespace, target.Name, minCPU, float64(d.policy.IdleCPUThreshold))
	} else {
		log.Info().Msgf("[DESCHEDULER] No idle pod under threshold found on %s", nodeName)
	}

	log.Debug().Msgf("[DESCHEDULER] Finished checking %d pods on %s (threshold=%.dm)", 
		len(pods.Items), nodeName, d.policy.IdleCPUThreshold)

	return target, nil
}

