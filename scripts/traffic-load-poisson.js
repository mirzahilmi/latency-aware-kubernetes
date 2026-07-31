import http from "k6/http";
import { SharedArray } from 'k6/data';
import { check } from "k6";

// Poisson variant of traffic-load.js.
//
// Same open-model semantics and the same mean load. Traffic is described as a
// list of segments; each segment holds one mean arrival rate lambda (req/s) per
// node for a fixed duration. Instead of k6 pacing arrivals perfectly evenly,
// the request count of every tick (default 1s) is drawn from
// Poisson(lambda * tick). The sampled per-tick rates are emitted as constant
// ramping-arrival-rate stages, so k6 keeps reporting dropped iterations under
// saturation.
//
// One segment (DISTRIBUTIONS + DURATION) is a homogeneous Poisson process:
// lambda is constant, only the per-tick counts fluctuate. Several segments
// (STEPS + STEP_DURATION) make lambda a step function of time, i.e. a
// non-homogeneous Poisson process. A staircase that rises and falls back to its
// starting level visits every load level twice, so the same offered lambda can
// be compared on the way up and on the way down -- that difference is the
// control loop's hysteresis.
//
// Because the whole trace comes from one seeded PRNG stream per node, a level
// revisited later in the run is an independent draw from the same distribution,
// not a replay of the earlier one.
//
// The PRNG is seeded (k6's Math.random cannot be), so a given SEED plus the
// same segment list and TICK always replays the exact same arrival trace. Run
// every configuration with the same SEED to keep the traffic identical across
// comparisons.
//
// Env (STEPS wins over DISTRIBUTIONS when both are set):
//   DISTRIBUTIONS  comma separated mean req/s, positional to config.json nodes[]
//   DURATION       k6 duration for DISTRIBUTIONS, e.g. "7m"
//   STEPS          semicolon separated DISTRIBUTIONS vectors, one per step
//   STEP_DURATION  optional  k6 duration held per step, default "3m"
//   SEED           optional  integer, default 42
//   TICK           optional  resampling interval, default "1s"
//   VU_FACTOR      optional  VUs per req/s of peak rate, default 1. This is a
//                            response time budget in seconds (Little's law): 1
//                            reserves enough VUs for 1s responses. Every VU is a
//                            JS runtime, so a loose factor is how a long run gets
//                            OOM-killed. The resolved allocation is logged before
//                            k6 starts allocating.

const nodes = new SharedArray("nodes", function () {
  return JSON.parse(open("./config.json")).nodes;
});

// mulberry32: small seedable PRNG, uniform in [0,1).
function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Lanczos approximation, needed by the rejection step below.
const LANCZOS = [
  0.99999999999980993, 676.5203681218851, -1259.1392167224028,
  771.32342877765313, -176.61502916214059, 12.507343278686905,
  -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7,
];

function logGamma(x) {
  const z = x - 1;
  const t = z + 7.5;
  let a = LANCZOS[0];
  for (let i = 1; i < LANCZOS.length; i++) a += LANCZOS[i] / (z + i);
  return 0.5 * Math.log(2 * Math.PI) + (z + 0.5) * Math.log(t) - t + Math.log(a);
}

// Knuth for small lambda, Hoermann's PTRS transformed rejection above 30
// (exp(-lambda) underflows there, and PTRS is O(1) instead of O(lambda)).
function samplePoisson(rand, lambda) {
  if (lambda <= 0) return 0;

  if (lambda < 30) {
    const limit = Math.exp(-lambda);
    let k = 0;
    let p = 1;
    do {
      k++;
      p *= rand();
    } while (p > limit);
    return k - 1;
  }

  const b = 0.931 + 2.53 * Math.sqrt(lambda);
  const a = -0.059 + 0.02483 * b;
  const invAlpha = 1.1239 + 1.1328 / (b - 3.4);
  const vr = 0.9277 - 3.6224 / (b - 2);
  const logLambda = Math.log(lambda);

  for (;;) {
    const u = rand() - 0.5;
    const v = rand();
    const us = 0.5 - Math.abs(u);
    const k = Math.floor(((2 * a) / us + b) * u + lambda + 0.43);

    if (us >= 0.07 && v <= vr) return k;
    if (k < 0 || (us < 0.013 && v > us)) continue;
    if (Math.log((v * invAlpha) / (a / (us * us) + b)) <=
        k * logLambda - lambda - logGamma(k + 1))
      return k;
  }
}

function parseDurationSeconds(value) {
  const text = String(value).trim();
  if (/^\d+(\.\d+)?$/.test(text)) return Number(text);

  const units = { h: 3600, m: 60, s: 1, ms: 0.001 };
  const re = /(\d+(?:\.\d+)?)(ms|h|m|s)/g;
  let total = 0;
  let consumed = 0;
  let part;
  while ((part = re.exec(text)) !== null) {
    total += Number(part[1]) * units[part[2]];
    consumed += part[0].length;
  }
  if (consumed !== text.length || total <= 0) throw `INVALID DURATION: ${value}`;
  return total;
}

// "800,400,400,200" -> [800, 400, 400, 200], validated against config.json.
//
// 0 is allowed and means "this node receives no traffic during this step": a
// Poisson process with lambda 0 simply produces no arrivals. That is how a
// single-entry-node testcase is written once it has to share a step list with
// four-node ones -- "200,0,0,0" rather than "200" -- since every step has to
// name the same nodes.
function parseLambdas(text, label) {
  const lambdas = text.split(",");

  if (lambdas.length > nodes.length)
    throw `${label} HAS MORE ENTRIES THAN config.json nodes[]: ${text}`;

  return lambdas.map(function (entry, i) {
    // Number("") is 0, so an empty field would otherwise pass as a silent zero.
    if (entry.trim() === "") throw `EMPTY RATE AT INDEX ${i} OF ${label}: ${text}`;
    const lambda = Number(entry);
    if (!Number.isFinite(lambda) || lambda < 0)
      throw `INVALID RATE AT INDEX ${i} OF ${label}: ${entry}`;
    return lambda;
  });
}

// Normalize both env shapes into [{ lambdas, duration }, ...].
function buildSegments() {
  if (nodes.length == 0) throw "config.json nodes[] IS EMPTY";

  if (__ENV.STEPS) {
    const durationText = __ENV.STEP_DURATION || "3m";
    // A trailing ";" is easy to leave behind when generating the list in shell.
    const steps = __ENV.STEPS.split(";").filter(function (step) {
      return step !== "";
    });
    if (steps.length == 0) throw "STEPS IS EMPTY";

    const segments = steps.map(function (step, s) {
      return {
        lambdas: parseLambdas(step, `STEPS[${s}]`),
        duration: durationText,
      };
    });

    // Every step drives the same set of scenarios, so a step that names fewer
    // nodes than another would silently leave that node idle for its window.
    for (let s = 1; s < segments.length; s++)
      if (segments[s].lambdas.length != segments[0].lambdas.length)
        throw `STEPS[${s}] HAS ${segments[s].lambdas.length} RATES BUT STEPS[0] HAS ${segments[0].lambdas.length}`;

    return segments;
  }

  if (!__ENV.DISTRIBUTIONS) throw "NEITHER STEPS NOR DISTRIBUTIONS IS SET";
  if (!__ENV.DURATION) throw "DURATION IS REQUIRED ALONGSIDE DISTRIBUTIONS";

  return [
    {
      lambdas: parseLambdas(__ENV.DISTRIBUTIONS, "DISTRIBUTIONS"),
      duration: __ENV.DURATION,
    },
  ];
}

function buildScenarios() {
  const tickText = __ENV.TICK || "1s";
  const tickSeconds = parseDurationSeconds(tickText);
  const seed = Number(__ENV.SEED || 42);
  if (!Number.isFinite(seed)) throw `INVALID SEED: ${__ENV.SEED}`;

  const segments = buildSegments();
  const ticksPerSegment = segments.map(function (segment) {
    return Math.max(1, Math.round(parseDurationSeconds(segment.duration) / tickSeconds));
  });

  // Zero is a legal rate per node per step, but a run where every rate is zero
  // would start k6, hold the whole schedule and send nothing.
  const busiest = segments.reduce(function (peak, segment) {
    return Math.max(peak, Math.max.apply(null, segment.lambdas));
  }, 0);
  if (busiest <= 0) throw "EVERY RATE IS ZERO; NO TRAFFIC WOULD BE SENT";

  const vuFactor = Number(__ENV.VU_FACTOR || 1);
  if (!(vuFactor > 0)) throw `INVALID VU_FACTOR: ${__ENV.VU_FACTOR}`;

  let totalVus = 0;
  const allocation = [];

  const scenarios = {};
  for (let i = 0; i < segments[0].lambdas.length; i++) {
    // One independent stream per node, still fully determined by SEED. Created
    // once for the whole run so the stream carries across segment boundaries.
    const rand = mulberry32(seed + i * 0x9e3779b1);

    const rates = [];
    for (let s = 0; s < segments.length; s++) {
      const lambda = segments[s].lambdas[i];
      for (let t = 0; t < ticksPerSegment[s]; t++)
        rates.push(Math.round(samplePoisson(rand, lambda * tickSeconds) / tickSeconds));
    }

    // Piecewise constant: instantaneous jump (0s) then hold for one tick.
    const stages = [{ target: rates[0], duration: tickText }];
    for (let t = 1; t < rates.length; t++) {
      stages.push({ target: rates[t], duration: "0s" });
      stages.push({ target: rates[t], duration: tickText });
    }

    let maxRate = 0;
    for (let t = 0; t < rates.length; t++)
      if (rates[t] > maxRate) maxRate = rates[t];

    // By Little's law a scenario needs lambda * E[response time] VUs in flight,
    // so VU_FACTOR is a response time budget in seconds. Sized for the peak of
    // the whole run, so the low steps of a staircase hold the peak allocation
    // the entire time -- growing the pool mid-run would charge VU startup to the
    // step that triggered it.
    //
    // Keep this tight. Every VU is a JS runtime, so an over-generous factor is
    // how a long run gets OOM-killed, and the pool is held for the whole run
    // rather than just the peak step. Capping it is also the CORRECT behaviour
    // for an open model rather than a workaround: once the cluster cannot keep
    // up, a bounded pool reports the shortfall as dropped_iterations, which is
    // the saturation signal. An unbounded one reports it as memory growth.
    const vus = Math.max(1, Math.ceil(maxRate * vuFactor));
    totalVus += vus;
    allocation.push(`${nodes[i].hostname}=${vus}`);

    scenarios[nodes[i].hostname] = {
      executor: "ramping-arrival-rate",
      startRate: rates[0],
      timeUnit: "1s",
      stages: stages,
      preAllocatedVUs: vus,
      maxVUs: vus,
      env: { TARGET: nodes[i].ip },
    };
  }

  // Printed before k6 starts allocating, so an allocation that will not fit is
  // an immediate abort rather than an OOM kill twenty minutes into the run.
  console.log(
    `VU allocation (VU_FACTOR=${vuFactor}): ${allocation.join(" ")} total=${totalVus}`
  );

  return scenarios;
}

// The init context re-runs for every VU, and the schedule can hold thousands of
// stages. Only the first pass (__VU == 0) is read for options, so build it once
// and hand VUs an empty object instead of a per-VU copy.
export const options = {
  scenarios: __VU == 0 ? buildScenarios() : {},
  discardResponseBodies: true,
  noVUConnectionReuse: true,
};

export default function() {
  const res = http.get(`http://${__ENV.TARGET}:30000`, { timeout: "300s" });
  check(res, { "status is 200": (res) => res.status === 200 });
}
