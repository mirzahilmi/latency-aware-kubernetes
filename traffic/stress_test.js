import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    { duration: '10m', target: 100 },
    { duration: '50m', target: 100 },
    { duration: '10m', target: 500 },
    { duration: '1h50m', target: 500 },
    { duration: '2h15m', target: 1500 },
    { duration: '1h45m', target: 1500 },
    { duration: '2h20m', target: 3000 },
    { duration: '3h40m', target: 3000 },
    { duration: '1h30m', target: 6000 },
    { duration: '4h30m', target: 6000 },
    { duration: '2h30m', target: 10000 },
    { duration: '2h30m', target: 10000 },
  ],
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET_HOSTNAME}:30000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
