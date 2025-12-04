package scheduler

import (
	"math"
	"strings"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
)

func ScoreNode(nodeName string, proberData map[string]prober.ScoreData, trafficNormMap map[string]float64, cfg ScoringConfig) int64 {
	ps, ok := proberData[nodeName]
	if !ok {
		return 0
	}

	lat := ps.LatencyEwmaScore
	cpu := ps.CPUEwmaScore
	mem := ps.MemoryEwmaScore
	
	traffic := trafficNormMap[nodeName]

	score:= (cfg.WeightLatency*lat + cfg.WeightCPU*cpu + cfg.WeightMem*mem + cfg.WeightTraffic*traffic) * 100
	return clampScore(score)
}

func ApplyCPUPenalty(nodeName string, cpuScore float64, cfg ScoringConfig) float64 {
	penalty := cfg.rpiPenaltyCPU
    if strings.Contains(nodeName, "vm") {
        penalty = cfg.vmPenaltyCPU
    } 

    penalized := cpuScore - penalty
    if penalized < 0 {
        penalized = 0
    }
    return penalized
}

func ApplyMemPenalty(nodeName string, memScore float64, cfg ScoringConfig) float64 {
	penalty := cfg.rpiPenaltyMem
    if strings.Contains(nodeName, "vm") {
        penalty = cfg.vmPenaltyMem
    } 

    penalized := memScore - penalty
    if penalized < 0 {
        penalized = 0
    }
    return penalized
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