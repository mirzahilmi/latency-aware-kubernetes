package descheduler

import (
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/scheduler"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/rs/zerolog/log"
)

// ScoreAllNodes count all node scores using extender.ScoreNode() formula
func (d *AdaptiveDescheduler) ScoreAllNodes(
	proberSlice []prober.ScoreData,
	trafficNorm map[string]float64,
	cfg extender.ScoringConfig,
) map[string]float64 {
	nodeScores := make(map[string]float64)
	for _, s := range proberSlice {
		nodeName := s.Hostname
		score := extender.ScoreNode(
			nodeName,
			map[string]prober.ScoreData{nodeName: s},
			trafficNorm,
			cfg,
		)
		nodeScores[nodeName] = float64(score)
		log.Debug().Msgf(
			"[DESCHEDULER] Node %s score=%.2f (CPU=%.3f Lat=%.3f Traffic=%.3f)",
			nodeName, float64(score), s.CPUEwmaScore, s.LatencyEwmaScore, trafficNorm[nodeName],
		)
	}

	return nodeScores
}
