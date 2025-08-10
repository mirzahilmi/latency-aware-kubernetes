import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    // Load
    { duration: '2m', target: 100 },
    { duration: '5m', target: 100 },
    { duration: '2m', target: 200 },
    { duration: '5m', target: 200 },

    // Stress
    { duration: '2m', target: 500 },
    { duration: '5m', target: 500 },
    { duration: '2m', target: 1000 },
    { duration: '5m', target: 1000 },
    { duration: '2m', target: 2000 },
    { duration: '5m', target: 2000 },

    // Spike
    { duration: '30s', target: 5000 },
    { duration: '1m', target: 5000 },
    { duration: '30s', target: 200 },
    { duration: '2m', target: 200 },
    { duration: '30s', target: 4000 },
    { duration: '1m', target: 4000 },
    { duration: '30s', target: 300 },
    { duration: '2m', target: 300 },

    // Soak
    { duration: '10m', target: 800 },
    { duration: '20h', target: 1000 },
    { duration: '30m', target: 0 },
  ]
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET_HOSTNAME}:8000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
