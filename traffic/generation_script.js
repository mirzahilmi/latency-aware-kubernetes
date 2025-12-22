import http from "k6/http";
import { SharedArray } from 'k6/data';
import { sleep, check } from "k6";

const nodes = new SharedArray("nodes", function () {
  const nodes = JSON.parse(open("./targets.json"));
  return nodes;
});
const distributions = __ENV.DISTRIBUTIONS.split(",");

if (nodes.length == 0 || distributions.length == 0)
  throw "NODES or DISTRIBUTIONS IS EMPTY";

var scenarios = {};
for (let i = 0; i < distributions.length; i++) {
  scenarios[nodes[i].hostname] = {
      executor: "constant-arrival-rate",
      duration: __ENV.DURATION,
      rate: Number(distributions[i]),
      preAllocatedVUs: Math.ceil(Number(distributions[i]) * 0.75),
      maxVUs: Number(distributions[i]) * 2,
      env: { TARGET: nodes[i].ip },
  };
}

export const options = {
  scenarios: scenarios,
  discardResponseBodies: true,
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET}:30002`, { timeout: "300s" });
  check(res, { "status is 200": (res) => res.status === 200 });
}
