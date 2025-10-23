package descheduler

import (
	"context"
	"fmt"
	"math"
	"os"
	"strings"

	"github.com/rs/zerolog/log"
	corev1 "k8s.io/api/core/v1"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
	"k8s.io/client-go/rest"
	metricsv "k8s.io/metrics/pkg/client/clientset/versioned"
)

func (d *Descheduler) findIdlePod(ctx context.Context, node string) (string, int64) {
	ns := os.Getenv("POD_NAMESPACE")

	// ambil daftar pod di node tersebut
	pods, err := d.clientset.CoreV1().Pods(ns).List(ctx,
		metav1.ListOptions{FieldSelector: fmt.Sprintf("spec.nodeName=%s", node)})
	if err != nil {
		log.Error().Err(err).Msgf("Failed to list pods on node %s", node)
		return "", 0
	}
	if len(pods.Items) == 0 {
		log.Warn().Msgf("No pods found on node %s", node)
		return "", 0
	}

	// buat metrics client
	config, err := rest.InClusterConfig()
	if err != nil {
		log.Warn().Err(err).Msg("Cannot load in-cluster config for metrics client")
		return d.pickAnyNonSystemPod(pods, ns)
	}
	metricsClient, err := metricsv.NewForConfig(config)
	if err != nil {
		log.Warn().Err(err).Msg("Cannot create metrics client")
		return d.pickAnyNonSystemPod(pods, ns)
	}

	// ambil data CPU usage dari metrics-server
	podMetricsList, err := metricsClient.MetricsV1beta1().PodMetricses(ns).List(ctx, metav1.ListOptions{})
	if err != nil {
		log.Warn().Err(err).Msg("Cannot get PodMetrics — fallback to random pod")
		return d.pickAnyNonSystemPod(pods, ns)
	}

	// mapping usage
	usageMap := make(map[string]int64)
	for _, m := range podMetricsList.Items {
		total := int64(0)
		for _, c := range m.Containers {
			if cpu := c.Usage[corev1.ResourceCPU]; !cpu.IsZero() {
				total += cpu.MilliValue()
			}
		}
		usageMap[m.Name] = total
	}

	// pilih pod dengan usage terkecil
	var idlePod string
	minUsage := int64(math.MaxInt64)
	for _, pod := range pods.Items {
		if isSystemPod(pod) {
			continue
		}
		u := usageMap[pod.Name]
		if u < minUsage {
			minUsage = u
			idlePod = pod.Name
		}
	}

	if idlePod == "" {
		log.Warn().Msgf("No idle pod detected on node %s, fallback to random", node)
		return d.pickAnyNonSystemPod(pods, ns)
	}

	log.Info().Msgf("[IdlePod] Selected %s (%.2fm) on node %s", idlePod, float64(minUsage), node)
	return idlePod, minUsage
}

func isSystemPod(p corev1.Pod) bool {
	if strings.Contains(p.Name, "scheduler") || strings.Contains(p.Name, "descheduler") || strings.Contains(p.Name, "metrics") {
		return true
	}
	for _, ref := range p.OwnerReferences {
		if ref.Kind == "DaemonSet" {
			return true
		}
	}
	return false
}

func (d *Descheduler) pickAnyNonSystemPod(pods *corev1.PodList, ns string) (string, int64) {
	for _, p := range pods.Items {
		if !isSystemPod(p) {
			log.Warn().Msgf("Fallback: selected random pod %s", p.Name)
			return p.Name, 0
		}
	}
	return "", 0
}
