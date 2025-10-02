import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    { duration: '2m', target: 500 },
    { duration: '10m', target: 500 },
    { duration: '50m', target: 3000 },
    { duration: '1h45m', target: 6000 },
    { duration: '1h', target: 10000 },
  ],
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET_HOSTNAME}:30000`);
  check(res, { "status is 200": (res) => res.status === 200 });
  sleep(1);
}
