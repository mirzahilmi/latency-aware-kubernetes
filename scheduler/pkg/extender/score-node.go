package extender

import (
	"math"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
)

func ScoreNode(nodeName string, proberData map[string]prober.ScoreData, trafficMap map[string]float64) float64 {
	scoreData, ok := proberData[nodeName]
	if !ok {
		return 0
	}

	// // normalisasi traffic
	// maxTraffic := 0.0
	// for _, v := range trafficMap {
	// 	if v > maxTraffic {
	// 		maxTraffic = v
	// 	}
	// }
	// if maxTraffic == 0 {
	// 	maxTraffic = 1
	// }
	// rawTraffic := trafficMap[nodeName]
	// trafficNormalized := rawTraffic / maxTraffic
	// if trafficNormalized < 0 {
	// 	trafficNormalized = 0
	// }

	lat := scoreData.LatencyEwmaScore
	cpu := scoreData.CPUEwmaScore

	weighted := (WeightLatency*lat + WeightCPU*cpu) * SCALE_FACTOR
	final := int(math.Round(weighted))
	if final < 0 {
		final = 0
	} else if final > int(SCALE_FACTOR) {
		final = int(SCALE_FACTOR)
	}
	return float64(final)
}
