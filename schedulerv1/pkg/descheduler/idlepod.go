package descheduler

import (
	"context"
	"fmt"

	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

	"github.com/rs/zerolog/log"
)

// TODO: add logic to evict multiple idle pods until node CPU > threshold
func (d *AdaptiveDescheduler) getMostIdlePod(ctx context.Context, nodeName string) (*corev1.Pod, error) {

	node, err := d.kubeClient.CoreV1().Nodes().Get(ctx, nodeName, metav1.GetOptions{})
	if err != nil {
		return nil, err
	}

	nodeIP := ""
	for _, addr := range node.Status.Addresses {
		if addr.Type == corev1.NodeInternalIP {
			nodeIP = addr.Address
			break
		}
	}

	if nodeIP == "" {
		return nil, fmt.Errorf("node %s has no InternalIP", nodeName)
	}

	summary, err := d.fetchSummaryAPI(nodeIP)
	if err != nil {
		return nil, err
	}

	pods, err := d.kubeClient.CoreV1().Pods(d.namespace).
		List(ctx, metav1.ListOptions{FieldSelector: "spec.nodeName=" + nodeName})
	if err != nil {
		return nil, err
	}

	minCPU := 999999999.0
	var target *corev1.Pod
	var targetMem float64

	for _, pod := range pods.Items {
		if isSystemPod(pod) {
			continue
		}

		cpu, mem, ok := getPodUsageFromSummary(pod.Name, pod.Namespace, summary)
		if !ok {
			continue
		}

		log.Debug().Msgf("[SUMMARY-POD] Pod %s: CPU=%.2fm, Memory=%.2fMi", pod.Name, cpu, mem)

		if cpu < minCPU &&
			cpu < float64(d.deschedCfg.idleCpuThres) &&
			mem < float64(d.deschedCfg.idleMemThres) {

			minCPU = cpu
			targetMem = mem
			target = &pod
		}
	}

	if target != nil {
		log.Info().Msgf("[IDLE-POD] Selected idle pod via SUMMARY API: %s/%s (CPU=%.2fm, Memory=%.2fMi)",
			target.Namespace, target.Name, minCPU, targetMem)
	}

	return target, nil
}
