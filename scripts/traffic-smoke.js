import http from "k6/http";
import { SharedArray } from 'k6/data';
import { sleep, check } from "k6";

const nodes = new SharedArray("nodes", function () {
  const nodes = JSON.parse(open("./targets.json"));
  return nodes;
});

if (nodes.length == 0)
  throw "NODES IS EMPTY";

var scenarios = {};
scenarios[`${nodes[0].hostname}`] = {
  executor: "constant-vus",
  duration: "5m",
  vus: __ENV.VUS,
  env: { TARGET: nodes[0].ip },
};
export const options = {
  scenarios: scenarios,
  discardResponseBodies: true,
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET}:30002`);
  check(res, { "status is 200": (res) => res.status === 200 });
}
