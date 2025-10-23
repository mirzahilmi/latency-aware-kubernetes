package influx

import (
	"math/rand"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/rs/zerolog/log"
)

type SharedDummyTraffic struct {
	mu             sync.RWMutex
	trafficMap     map[string]float64
	topNode        string
	topValue       float64
	lastUpdate     time.Time
	updateInterval time.Duration
	nodeNames      []string
}

var (
	sharedInstance *SharedDummyTraffic
	sharedOnce     sync.Once
)

// GetSharedDummyTraffic mengembalikan singleton instance
func GetSharedDummyTraffic() *SharedDummyTraffic {
	sharedOnce.Do(func() {
		// Parse node names dari environment variable
		nodesEnv := os.Getenv("DUMMY_NODES")
		var nodes []string
		if nodesEnv != "" {
			nodes = strings.Split(nodesEnv, ",")
			for i := range nodes {
				nodes[i] = strings.TrimSpace(nodes[i])
			}
		} else {
			// Default nodes
			nodes = []string{
				"k8s-slave-1-raspberrypi-4",
				"k8s-slave-2-vm",
				"k8s-slave-3-raspberrypi-4",
				"k8s-slave-4-raspberrypi-4",
			}
		}

		intervalStr := os.Getenv("DUMMY_UPDATE_INTERVAL")
		interval := 90 * time.Second
		if intervalStr != "" {
			if parsed, err := time.ParseDuration(intervalStr); err == nil {
				interval = parsed
			}
		}

		sharedInstance = &SharedDummyTraffic{
			trafficMap:     make(map[string]float64),
			nodeNames:      nodes,
			updateInterval: interval,
		}

		sharedInstance.initializeTraffic()

		go sharedInstance.backgroundUpdater()
	})
	return sharedInstance
}

func (sdt *SharedDummyTraffic) initializeTraffic() {
	sdt.mu.Lock()
	defer sdt.mu.Unlock()

	baseTrafficStr := os.Getenv("DUMMY_BASE_TRAFFIC")
	baseTraffic := 100.0
	if baseTrafficStr != "" {
		if parsed, err := strconv.ParseFloat(baseTrafficStr, 64); err == nil {
			baseTraffic = parsed
		}
	}

	varianceStr := os.Getenv("DUMMY_TRAFFIC_VARIANCE")
	variance := 50.0
	if varianceStr != "" {
		if parsed, err := strconv.ParseFloat(varianceStr, 64); err == nil {
			variance = parsed
		}
	}

	for _, node := range sdt.nodeNames {
		sdt.trafficMap[node] = baseTraffic + rand.Float64()*100
	}

	sdt.updateTopNode()
	sdt.lastUpdate = time.Now()

	log.Info().
		Int("nodeCount", len(sdt.nodeNames)).
		Float64("baseTraffic", baseTraffic).
		Float64("variance", variance).
		Msg("[SharedDummy] Initialized traffic data")
}

// backgroundUpdater memperbarui traffic secara periodik
func (sdt *SharedDummyTraffic) backgroundUpdater() {
	ticker := time.NewTicker(sdt.updateInterval)
	defer ticker.Stop()

	for range ticker.C {
		sdt.update()
	}
}

func (sdt *SharedDummyTraffic) update() {
	sdt.mu.Lock()
	defer sdt.mu.Unlock()

	// Parse spike probability dari env
	spikeProbStr := os.Getenv("DUMMY_SPIKE_PROB")
	spikeProb := 0.3
	if spikeProbStr != "" {
		if parsed, err := strconv.ParseFloat(spikeProbStr, 64); err == nil {
			spikeProb = parsed
		}
	}

	// Update setiap node dengan perubahan kecil
	for node := range sdt.trafficMap {
		// Random walk: ±50 req/min
		change := (rand.Float64() - 0.5) * 100
		newVal := sdt.trafficMap[node] + change

		// Pastikan tidak negatif dan tidak terlalu rendah
		if newVal < 50 {
			newVal = 50 + rand.Float64()*50
		}

		sdt.trafficMap[node] = newVal
	}

	// Random spike untuk simulasi burst traffic
	if rand.Float64() < spikeProb {
		targetNode := sdt.nodeNames[rand.Intn(len(sdt.nodeNames))]
		spikeAmount := 300 + rand.Float64()*300
		sdt.trafficMap[targetNode] += spikeAmount

		log.Info().
			Str("node", targetNode).
			Float64("spike", spikeAmount).
			Msg("[SharedDummy] Traffic spike generated")
	}

	sdt.updateTopNode()
	sdt.lastUpdate = time.Now()

	log.Info().Msg("[SharedDummy] Updated traffic map:")
	for _, node := range sdt.nodeNames {
		log.Info().Msgf("   %s → %.2f req/min", node, sdt.trafficMap[node])
	}
	log.Info().Msgf("[SharedDummy] topNode: %s (%.2f req/min)", sdt.topNode, sdt.topValue)
}

// updateTopNode mencari node dengan traffic tertinggi (harus dipanggil dengan lock)
func (sdt *SharedDummyTraffic) updateTopNode() {
	maxNode := ""
	maxVal := 0.0

	for node, val := range sdt.trafficMap {
		if val > maxVal {
			maxNode = node
			maxVal = val
		}
	}

	sdt.topNode = maxNode
	sdt.topValue = maxVal
}

// GetTopNode mengembalikan node dengan traffic tertinggi
func (sdt *SharedDummyTraffic) GetTopNode() (string, float64) {
	sdt.mu.RLock()
	defer sdt.mu.RUnlock()
	return sdt.topNode, sdt.topValue
}

// GetTrafficMap mengembalikan copy dari traffic map
func (sdt *SharedDummyTraffic) GetTrafficMap() map[string]float64 {
	sdt.mu.RLock()
	defer sdt.mu.RUnlock()

	result := make(map[string]float64)
	for k, v := range sdt.trafficMap {
		result[k] = v
	}
	return result
}

// GetNodeTraffic mengembalikan traffic untuk node tertentu
func (sdt *SharedDummyTraffic) GetNodeTraffic(nodeName string) (float64, bool) {
	sdt.mu.RLock()
	defer sdt.mu.RUnlock()

	val, ok := sdt.trafficMap[nodeName]
	return val, ok
}

// ForceUpdate memaksa update traffic (untuk testing)
func (sdt *SharedDummyTraffic) ForceUpdate() {
	sdt.update()
}

// func generateDummyTraffic() (map[string]float64, string, float64, error) {
// 	dummyInitOnce.Do(func() {
// 		dummyTrafficMap = map[string]float64{
// 			"k8s-slave-1-raspberrypi-4": 200 + rand.Float64()*100,
// 			"k8s-slave-2-vm":            150 + rand.Float64()*80,
// 			"k8s-slave-3-raspberrypi-4": 120 + rand.Float64()*60,
// 			"k8s-slave-4-raspberrypi-4": 140 + rand.Float64()*70,
// 		}
// 		updateDummyTraffic() // inisialisasi pertama
// 	})

// 	// hanya update kalau sudah lewat 30 detik
// 	updateDummyTraffic()

// 	dummyMu.RLock()
// 	defer dummyMu.RUnlock()

// 	// bikin salinan biar aman
// 	trafficCopy := make(map[string]float64)
// 	for k, v := range dummyTrafficMap {
// 		trafficCopy[k] = v
// 	}
// 	return trafficCopy, dummyTopNode, dummyTopValue, nil
// }

// func updateDummyTraffic() {
// 	dummyMu.Lock()
// 	defer dummyMu.Unlock()

// 	// kalau baru update <30 detik lalu, skip — supaya konsisten
// 	if time.Since(dummyLastUpdate) < 90*time.Second {
// 		return
// 	}

// 	for node := range dummyTrafficMap {
// 		change := (rand.Float64() - 0.5) * 100 // ±50 perubahan
// 		newVal := dummyTrafficMap[node] + change
// 		if newVal < 30 {
// 			newVal = 30
// 		}
// 		dummyTrafficMap[node] = newVal
// 	}

// 	// kemungkinan spike
// 	if rand.Float64() < 0.4 {
// 		nodes := []string{
// 			"k8s-slave-1-raspberrypi-4",
// 			"k8s-slave-2-vm",
// 			"k8s-slave-3-raspberrypi-4",
// 			"k8s-slave-4-raspberrypi-4",
// 		}
// 		target := nodes[rand.Intn(len(nodes))]
// 		dummyTrafficMap[target] += 300 + rand.Float64()*200
// 	}

// 	// hitung top node baru
// 	maxNode := ""
// 	maxVal := 0.0
// 	for n, v := range dummyTrafficMap {
// 		if v > maxVal {
// 			maxNode, maxVal = n, v
// 		}
// 	}
// 	dummyTopNode, dummyTopValue = maxNode, maxVal
// 	dummyLastUpdate = time.Now()

// 	log.Info().Msgf("[Dummy] Updated traffic map:")
// 	for n, v := range dummyTrafficMap {
// 		log.Info().Msgf("   %s → %.2f req/min", n, v)
// 	}
// 	log.Info().Msgf("[Dummy] topNode: %s (%.2f req/min)", dummyTopNode, dummyTopValue)
// }
