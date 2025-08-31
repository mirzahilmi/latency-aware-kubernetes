import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    // Load
    { duration: "2m", target: 150 },
    { duration: "5m", target: 150 },
    { duration: "2m", target: 250 },
    { duration: "5m", target: 250 },
    // Stress
    { duration: "2m", target: 300 },
    { duration: "5m", target: 300 },
    { duration: "2m", target: 450 },
    { duration: "5m", target: 450 },
    { duration: "2m", target: 600 },
    { duration: "5m", target: 600 },
    // Spike
    { duration: "30s", target: 800 },
    { duration: "1m", target: 800 },
    { duration: "30s", target: 100 },
    { duration: "2m", target: 100 },
    { duration: "30s", target: 900 },
    { duration: "1m", target: 900 },
    { duration: "30s", target: 250 },
    { duration: "2m", target: 250 },
    // Soak
    { duration: "10m", target: 150 },
    { duration: "20h", target: 750 },
    { duration: "30m", target: 0 },
  ],
};

export default function() {
  const res = http.post(`http://${__ENV.TARGET_HOSTNAME}:30000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
