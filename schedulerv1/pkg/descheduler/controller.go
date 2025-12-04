package descheduler

import (
	"context"
	"time"

	"github.com/rs/zerolog/log"
	"k8s.io/apimachinery/pkg/util/wait"
)

func NewController(desched *AdaptiveDescheduler) *Controller {
    return &Controller{
        descheduler: desched,
        config: desched.deschedCfg,
    }
}

func (c *Controller) Run(ctx context.Context) {
    interval := time.Duration(c.config.interval) * time.Second
	    log.Info().Msgf("[DESCHEDULER] Running every %fs", c.config.interval)
	
    	wait.Until(func() {
		c.descheduler.evaluate(ctx)
	}, interval, ctx.Done())
}
