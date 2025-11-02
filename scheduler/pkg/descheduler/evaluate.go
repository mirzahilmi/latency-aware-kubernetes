package descheduler

import (
	"context"
	"math"
	"time"

	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/extender"
	"github.com/mirzahilmi/latency-aware-kubernetes/scheduler/pkg/prober"
	"github.com/rs/zerolog/log"
)

func (d *AdaptiveDescheduler) Run(ctx context.Context) {
	intervalSec := d.policy.IntervalSeconds

	log.Info().Msgf("[DESCHEDULER] Starting adaptive descheduler loop (interval=%ds)", intervalSec)

	ticker := time.NewTicker(time.Duration(intervalSec) * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			log.Warn().Msg("[DESCHEDULER] Context canceled, stopping loop")
			return
		case <-ticker.C:
			d.evaluateCycle(ctx)
		}
	}

}

func (d *AdaptiveDescheduler) evaluateCycle(ctx context.Context) {
	log.Debug().Msg("[DESCHEDULER] Evaluating cluster state...")

	//get topNode
	topNode, rate, err := d.influxService.QueryTopNode(d.bucket)
	if err != nil {
		log.Error().Err(err).Msg("[DESCHEDULER] Failed to query topNode")
		return
	}
	if topNode == "" {
		log.Warn().Msg("[DESCHEDULER] No topNode detected — skipping")
		return
	}
	log.Info().Msgf("[DESCHEDULER] Current topNode: %s (%.2f req/min)", topNode, rate)

	// check if new top node = previous top node
	if d.prevTopNode == "" {
		d.prevTopNode = topNode
		log.Info().Msg("[DESCHEDULER] Initial topNode recorded")
		return
	}
	if d.prevTopNode == topNode {
		log.Debug().Msg("[DESCHEDULER] Traffic stable — no eviction this cycle")
		return
	}
	log.Info().Msgf("[DESCHEDULER] Traffic shift detected (%s → %s)", d.prevTopNode, topNode)

	// get prober & traffic data
	proberData, err := prober.FetchScoresFromNode(topNode)
	if err != nil {
		log.Warn().Err(err).Msg("[DESCHEDULER] Failed to fetch prober data — skipping")
		return
	}

	proberMap := make(map[string]prober.ScoreData)
	for _, s := range proberData {
		proberMap[s.Hostname] = s
	}

	trafficNorm, err := d.influxService.NormalizedTraffic(d.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[DESCHEDULER] Failed to fetch normalized traffic")
		trafficNorm = map[string]float64{}
	}

	cfg := extender.LoadScoringConfig()
	nodeScores := d.ScoreAllNodes(proberData, trafficNorm, cfg)

	// find the worst node
	worstNode := ""
	lowestScore := math.MaxFloat64
	for node, score := range nodeScores {
		if float64(score) < lowestScore {
			lowestScore = float64(score)
			worstNode = node
		}
	}
	if worstNode == "" {
		log.Warn().Msg("[DESCHEDULER] No valid worst node found — skipping eviction")
		return
	}

	log.Warn().Msgf("[DESCHEDULER] Worst node identified: %s (score=%.2f)", worstNode, lowestScore)

	// Evict idle pod in worst node
	if err := d.evictIdlePod(ctx, worstNode); err != nil {
		log.Warn().Err(err).Msg("[DESCHEDULER] Eviction failed")
	} else {
		log.Info().Msgf("[DESCHEDULER] Evicted idle pod from %s", worstNode)
	}

	d.prevTopNode = topNode
}


