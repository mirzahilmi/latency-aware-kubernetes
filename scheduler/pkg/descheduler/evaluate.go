package descheduler

import (
	"context"

	"github.com/rs/zerolog/log"
)

func (d *Descheduler) Evaluate(ctx context.Context) {
	d.mu.Lock()
	defer d.mu.Unlock()

	topNode, maxVal, err := d.influxService.QueryTopNode(d.bucket)
	if err != nil {
		log.Warn().Err(err).Msg("[DESCHEDULER] Failed to query top node from Influx")
		return
	}
	if topNode == "" {
		log.Warn().Msg("[DESCHEDULER] No top node found in query result")
		return
	}

	if d.prevTopNode == "" {
		d.prevTopNode = topNode
		log.Info().Msgf("[DESCHEDULER] Initialize prevTopNode: %s", topNode)
		return
	}

	if d.prevTopNode == topNode {
		log.Info().Msgf("[DESCHEDULER] Top node unchanged: %s", topNode)
	}

	// if topNOde changes -> triggering pod evictions
	if topNode != d.prevTopNode {
		log.Warn().Msgf("[DESCHEDULER] Top node changed: %s → %s (%.2f req/min)",
			d.prevTopNode, topNode, maxVal)

		// Evict idle pod from old topNode
		d.evictPodsForRebalancing(ctx, d.prevTopNode)

		// Update prevTopNode
		d.prevTopNode = topNode
	} else {
		log.Info().Msgf("[DESCHEDULER] Top node unchanged (%s) — no action", topNode)
	}
}
