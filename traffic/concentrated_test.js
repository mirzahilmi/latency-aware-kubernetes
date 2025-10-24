import http from 'k6/http';
import { sleep, check } from 'k6';

export const options = {
  stages: [
    { duration: '30m', target: Number(__ENV.VUS) },
  ],
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET}:30000`);
  check(res, { "status is 200": (res) => res.status === 200 });
}
