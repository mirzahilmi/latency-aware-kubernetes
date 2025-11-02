package extender

import (
	"math"
	"strings"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
)

func ScoreNode(nodeName string, proberData map[string]prober.ScoreData, trafficNormMap map[string]float64, cfg ScoringConfig) int64 {
	scoreData, ok := proberData[nodeName]
	if !ok {
		return 0
	}

	lat := scoreData.LatencyEwmaScore
	cpu := scoreData.CPUEwmaScore
	traffic := trafficNormMap[nodeName]

	score:= (cfg.WeightLatency*lat + cfg.WeightCPU*cpu + cfg.WeightTraffic*traffic) * cfg.ScaleFactor

	return clampScore(score)
}

func clampScore(s float64) int64 {
	if s < 0 {
		s = 0
	}
	if s > 100 {
		s = 100
	}
	return int64(math.Round(s))
}

func ApplyCPUPenalty(nodeName string, cpuScore float64) float64 {
    penalty := 0.0
	// applying hardcoded cpu penalty here
    if strings.Contains(nodeName, "vm") {
        penalty = 0.10 
    } else {
        penalty = 0.05 
    }

    penalized := cpuScore - penalty
    if penalized < 0 {
        penalized = 0
    }
    return penalized
}