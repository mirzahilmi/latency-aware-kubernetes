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
	ticker := time.NewTicker(time.Duration(d.policy.IntervalSeconds) * time.Second)
	defer ticker.Stop()

	log.Info().Msgf("[DESCHEDULER] Starting adaptive descheduler loop (interval=%ds)", d.policy.IntervalSeconds)

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
	topNode, rate, err := d.influxService.QueryTopNode(d.bucket)
	if err != nil {
		log.Error().Err(err).Msg("[DESCHEDULER] Failed to query topNode")
		return
	}

	if topNode == "" {
		log.Warn().Msg("[DESCHEDULER] No topNode detected — skipping evaluation")
		return
	}

	cfg := d.config
	log.Info().Msgf("[DESCHEDULER] Current topNode: %s (%.2f req/min)", topNode, rate)

	//condition check, descheduler will evict pod if:
	//1. new top node has traffic > threshold
	if rate < cfg.WarmupThreshold {
        log.Warn().Msgf("[DESCHEDULER] Traffic %.2f < %.2f, skipping evaluation (cluster cold-start or idle)", rate, cfg.WarmupThreshold)
        return
    }
	//2. new top node != previous top node
	switch {
	case d.prevTopNode == "":
		d.prevTopNode = topNode
		log.Info().Msgf("[DESCHEDULER] Initial topNode recorded: %s (%.2f req/min)", topNode, rate)
		return
	case d.prevTopNode == topNode:
		log.Debug().Msgf("[DESCHEDULER] Traffic stable, topNode unchanged (%s)", topNode)
		return
	}

    log.Info().Msgf("[DESCHEDULER] Traffic shift detected (%s → %s, rate=%.2f)", d.prevTopNode, topNode, rate)

	proberData, err := prober.FetchScoresFromNode(topNode)
	if err != nil || len(proberData) == 0 {
		log.Warn().Err(err).Msgf("[DESCHEDULER] Failed to fetch prober data from %s", topNode)
		return
	}

	trafficNorm, err := d.influxService.NormalizedTraffic(d.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[DESCHEDULER] Failed to fetch normalized traffic")
		trafficNorm = map[string]float64{}
	}

	// Compute node scores
	nodeScores := make(map[string]float64)
    for _, s := range proberData {
        node := s.Hostname
        nodeScores[node] = float64(extender.ScoreNode(node, map[string]prober.ScoreData{node: s}, trafficNorm, cfg))
    }

	// find the worst node
	worstNode, lowestScore := "", math.MaxFloat64
	for node, score := range nodeScores {
		if score < lowestScore {
			worstNode, lowestScore = node, score
		}
	}

	if worstNode == "" {
		log.Warn().Msg("[DESCHEDULER] No target node selected, skipping eviction")
		return
	}

	log.Warn().Msgf("[DESCHEDULER] Worst node identified: %s (score=%.2f)", worstNode, lowestScore)

	// Evict idle pod in worst node
	log.Info().Msgf("[DESCHEDULER] Searching for idle pods on %s (threshold=%.dm)",
	worstNode, d.policy.IdleCPUThreshold)

	if err := d.evictIdlePod(ctx, worstNode); err != nil {
		log.Warn().Err(err).Msgf("[DESCHEDULER] Eviction process failed for %s", worstNode)
	} else {
		log.Info().Msgf("[DESCHEDULER] Evicted idle pod(s) from %s due to traffic shift", worstNode)
	}
	d.prevTopNode = topNode
}

