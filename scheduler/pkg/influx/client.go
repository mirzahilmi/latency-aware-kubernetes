// pkg/influx/client.go
package influx

import (
	"context"
	"os"
	"time"

	influxdb2 "github.com/influxdata/influxdb-client-go/v2"
	"github.com/influxdata/influxdb-client-go/v2/api"
	"github.com/rs/zerolog/log"
)

type Service struct {
	client   influxdb2.Client
	queryAPI api.QueryAPI
	org      string
	bucket   string
}

// NewService membuat InfluxDB service untuk production
func NewService() *Service {
	url := os.Getenv("INFLUX_HOST")
	token := os.Getenv("INFLUX_TOKEN")
	org := os.Getenv("INFLUX_ORG")
	bucket := os.Getenv("INFLUX_BUCKET")

	log.Info().
		Str("url", url).
		Str("org", org).
		Str("bucket", bucket).
		Msg("Connecting to InfluxDB...")

	// Buat client
	client := influxdb2.NewClient(url, token)

	// Test connection
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	health, err := client.Health(ctx)
	if err != nil {
		log.Fatal().
			Err(err).
			Str("url", url).
			Msg("Failed to connect to InfluxDB")
	}

	log.Info().
		Str("status", string(health.Status)).
		Str("message", *health.Message).
		Msg("InfluxDB connection successful")

	queryAPI := client.QueryAPI(org)

	return &Service{
		client:   client,
		queryAPI: queryAPI,
		org:      org,
		bucket:   bucket,
	}
}

func (s *Service) Close() {
	if s.client != nil {
		s.client.Close()
		log.Info().Msg("InfluxDB client closed")
	}
}

// GetBucket returns bucket name
func (s *Service) GetBucket() string {
	return s.bucket
}