package descheduler

import (
	"context"
	"math"

	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	metricsv1beta1 "k8s.io/metrics/pkg/apis/metrics/v1beta1"

	"github.com/rs/zerolog/log"
)
//TODO: add memory metric check for evicting idle pods
//TODO: add logic to evict multiple idle pods until node CPU > threshold
func (d *AdaptiveDescheduler) getMostIdlePod(ctx context.Context, nodeName string) (*corev1.Pod, error) {
	pods, err := d.kubeClient.CoreV1().Pods(d.namespace).
		List(ctx, metav1.ListOptions{FieldSelector: "spec.nodeName=" + nodeName})
	if err != nil {
		return nil, err
	}

	if len(pods.Items) == 0 {
		log.Debug().Msgf("[IDLE-POD] No pods found on %s", nodeName)
		return nil, nil
	}

	podMetrics, err := d.metricsClient.MetricsV1beta1().
		PodMetricses(d.namespace).List(ctx, metav1.ListOptions{})
	if err != nil {
		return nil, err
	}

	minCPU := math.MaxFloat64
	var target *corev1.Pod

	for _, pod := range pods.Items {
		if isSystemPod(pod) {
			continue
		}

		cpu := collectPodCPU(pod.Name, podMetrics)
        if cpu < 0 {
            continue 
        }

		log.Debug().Msgf("[IDLE-POD] Pod %s: CPU=%.2fm", pod.Name, cpu)

		if cpu < minCPU && cpu < float64(d.deschedCfg.idleCpuThres) {
            minCPU = cpu
            target = &pod
        }
    }

	if target != nil {
		log.Info().Msgf(
			"[IDLE-POD] Selected idle pod: %s/%s (CPU=%.2fm < threshold=%.2fm)",
			target.Namespace, target.Name, minCPU, d.deschedCfg.idleCpuThres,
		)
	} else {
		log.Info().Msgf("IDLE-POD] No idle pod found under threshold (%.2fm) on %s", d.deschedCfg.idleCpuThres, nodeName)
	}

	return target, nil
}

func collectPodCPU(podName string, metrics *metricsv1beta1.PodMetricsList) float64 {
    for _, m := range metrics.Items {
        if m.Name != podName {
            continue
        }

        var total float64
        for _, c := range m.Containers {
            q := c.Usage[corev1.ResourceCPU]
            total += float64(q.MilliValue())
        }
        return total
    }
    return -1
}



