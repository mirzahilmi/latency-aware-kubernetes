package logging

import (
	"os"

	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
	stdlog "log"
)

func Configure() {
	logLevel := os.Getenv("LOG_LEVEL")
	if logLevel == "" {
		stdlog.Panicf("logging: missing LOG_LEVEL env var")
	}
	log.Logger = zerolog.New(os.Stdout).
		With().
		Stack().
		Timestamp().Logger()

	level, err := zerolog.ParseLevel(logLevel)
	if err != nil {
		stdlog.Panicf(`logging: failed to parse log level of %s: %v`, logLevel, err)
	}
	zerolog.SetGlobalLevel(level)
}
