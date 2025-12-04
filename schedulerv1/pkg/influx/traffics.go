package influx

import (
	"context"
	"fmt"
	"math"

	"github.com/rs/zerolog/log"
)

// QueryTrafficByNode returns map[node_name] = req_per_min
func (s *Service) QueryTrafficByNode(bucket string) (map[string]float64, error) {
	if s.client == nil || s.queryAPI == nil {
		return nil, fmt.Errorf("[INFLUX] client or queryAPI not initialized")
	}

	query := fmt.Sprintf(`
	from(bucket: "%s")
	|> range(start: -2m)
	|> filter(fn: (r) => r["_measurement"] == "http_packet")
	|> filter(fn: (r) => r["_field"] == "counter")
	|> aggregateWindow(every: 1m, fn: last, createEmpty: false)
	|> derivative(unit: 1m, nonNegative: true)
	|> group(columns: ["node_name"])
	`, bucket)

	result, err := s.queryAPI.Query(context.Background(), query)
	if err != nil {
		return nil, fmt.Errorf("[INFLUX] query error: %v", err)
	}
	defer result.Close()

	trafficMap := make(map[string]float64)

	for result.Next() {
		record := result.Record()

		node, _ := record.ValueByKey("node_name").(string)
		if node == "" {
			node, _ = record.ValueByKey("node").(string)
		}
		if node == "" {
			node, _ = record.ValueByKey("host").(string)
		}
		
		value, ok := record.Value().(float64)
		if !ok {
			log.Debug().Msg("[INFLUX] Cannot convert value to float64")
			continue
		}

		if node == "" {
			log.Debug().Msg("[INFLUX] Skipping record with empty node name")
			continue
		}
		
		if math.IsNaN(value) || math.IsInf(value, 0) {
			log.Debug().Msgf("[INFLUX] Skipping node %s: invalid value (NaN or Inf)", node)
			continue
		}
		
		if value < 0 {
			log.Debug().Msgf("[INFLUX] Skipping node %s: negative value (%.2f)", node, value)
			continue
		}
		trafficMap[node] = value
		//log.Debug().Msgf("[INFLUX] Node %s traffic: %.2f req/min", node, value) //deactivate
	}


	if err := result.Err(); err != nil {
		return nil, fmt.Errorf("[INFLUX] result iteration error: %v", err)
	}

	if len(trafficMap) == 0 {
		log.Warn().Msg("[INFLUX] No traffic data found, possible idle cluster or empty measurement")
	} else {
		log.Info().Msg("[INFLUX][TRAFFIC] Node traffic summary:")
		for node, val := range trafficMap {
			log.Info().Msgf("%s = %.2f req/min", node, val)
		}
	}

	return trafficMap, nil
}

func (s *Service) NormalizedTraffic(bucket string) (map[string]float64, error) {
	trafficMap, err := s.QueryTrafficByNode(bucket)
	if err != nil {
		return nil, err
	}
	if len(trafficMap) == 0 {
		return map[string]float64{}, nil
	}

	var maxTraffic float64
	for _, v := range trafficMap {
		if v > maxTraffic {
			maxTraffic = v
		}
	}

	normMap := make(map[string]float64)
	for node, val := range trafficMap {
		n := val / maxTraffic
		if n > 1 {
			n = 1
		}
		if n < 0 || math.IsNaN(n) {
			n = 0
		}
		normMap[node] = n
	}

	log.Info().Msg("[INFLUX][TRAFFIC] Normalized traffic (0–1 scale):")
	for node, n := range normMap {
		log.Info().Msgf("%s = %.3f", node, n)
	}

	return normMap, nil
}
