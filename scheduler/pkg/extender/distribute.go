package extender

import (
	"context"
	"sync"
	"time"

	"github.com/rs/zerolog/log"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"
)

// PodDistribution nyimpen jumlah pod per node
type PodDistribution struct {
	mu              sync.RWMutex
	podCountPerNode map[string]int
	minPodsPerNode  int
}

// NewPodDistribution inisialisasi distribusi baru
func NewPodDistribution(minPods int) *PodDistribution {
	return &PodDistribution{
		podCountPerNode: make(map[string]int),
		minPodsPerNode:  minPods,
	}
}

// GetPodCount balikin jumlah pod yang udah dijadwalin di node tertentu
func (d *PodDistribution) GetPodCount(node string) int {
	d.mu.RLock()
	defer d.mu.RUnlock()
	return d.podCountPerNode[node]
}

// AnyNodeHasZeroPod ngecek apakah masih ada node yang belum punya pod
func (d *PodDistribution) AnyNodeHasZeroPod() bool {
	d.mu.RLock()
	defer d.mu.RUnlock()
	for _, count := range d.podCountPerNode {
		if count == 0 {
			return true
		}
	}
	return false
}

// UpdateCount nambah / ngurangin jumlah pod di node tertentu
// delta = +1 kalau pod baru dijadwalin ke node tsb, -1 kalau pod dihapus
func (d *PodDistribution) UpdateCount(node string, delta int) {
	d.mu.Lock()
	defer d.mu.Unlock()
	d.podCountPerNode[node] += delta
	if d.podCountPerNode[node] < 0 {
		d.podCountPerNode[node] = 0
	}
}


func (e *Extender) refreshDistributionLoop() {
	refreshInterval := 30 * time.Second

	for {
		if e.clientset == nil {
			log.Warn().Msg("[EXTENDER] clientset is nil — distribution loop skipped")
			time.Sleep(refreshInterval)
			continue
		}

		podList, err := e.clientset.CoreV1().
			Pods(e.namespace).
			List(context.TODO(), metav1.ListOptions{})
		if err != nil {
			log.Warn().Err(err).Msg("[EXTENDER] Failed to list pods for distribution")
			time.Sleep(refreshInterval)
			continue
		}

		nodeCount := make(map[string]int)
		for _, pod := range podList.Items {
			// hanya hitung pod yang sedang dijadwalkan dan bukan terminated
			if pod.Spec.NodeName != "" && pod.Status.Phase == "Running" {
				nodeCount[pod.Spec.NodeName]++
			}
		}

		e.mu.Lock()
		e.distribution.podCountPerNode = make(map[string]int) // reset dulu
		for node, count := range nodeCount {
			e.distribution.UpdateCount(node, count)
		}
		e.mu.Unlock()

		log.Info().
			Int("nodesTracked", len(nodeCount)).
			Msg("[EXTENDER] Distribution cache refreshed")

		time.Sleep(refreshInterval)
	}
}