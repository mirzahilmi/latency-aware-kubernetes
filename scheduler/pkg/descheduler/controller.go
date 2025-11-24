package descheduler

import (
	"context"
	"time"

	"github.com/rs/zerolog/log"
)

func NewController(desched *AdaptiveDescheduler) *Controller {
    return &Controller{
        descheduler: desched,
        cfg: desched.deschedCfg,
    }
}

func (c *Controller) Run(ctx context.Context) {
    interval := time.Duration(c.cfg.interval) * time.Second
    ticker := time.NewTicker(interval)
    defer ticker.Stop()

    log.Info().Msgf("[DESCHEDULER] Running every %fs", c.cfg.interval)

    for {
        select {
        case <-ctx.Done():
            log.Info().Msg("[DESCHEDULER] Stopped")
            return

        case <-ticker.C:
            c.descheduler.evaluate(ctx)
        }
    }
}
