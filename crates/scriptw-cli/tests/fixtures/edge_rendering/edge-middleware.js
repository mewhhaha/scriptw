// Deterministic middleware records. Token lookup is authorization policy, not cryptography.
function run() {
  const now = 1700000000000;
  const buckets = new Map();
  function handle(request) {
    if (request.method === "OPTIONS") return respond(204, { preflight: true });
    const authorization = request.headers.authorization;
    let user;
    if (authorization === "Bearer alpha") user = "ada";
    if (authorization === "Bearer beta") user = "zoë";
    if (!user) return respond(401, { error: "authorization" });
    if (request.method !== "GET" || request.path === "/admin") return respond(403, { error: "policy" });
    const key = user + ":" + Math.floor(now / 60000);
    const used = buckets.get(key) || 0;
    if (used >= 2) return respond(429, { error: "rate", retryAfter: 60 });
    buckets.set(key, used + 1);
    const response = respond(200, { user, path: request.path, at: now });
    response.headers["x-rate-limit-remaining"] = String(1 - used);
    return response;
  }
  const inputs = [
    ["GET", "/data", "Bearer alpha"], ["GET", "/data", "Bearer beta"],
    ["POST", "/data", "Bearer alpha"], ["GET", "/data", ""],
    ["OPTIONS", "/data", ""], ["GET", "/data", "Bearer invalid"],
    ["GET", "/search", "Bearer alpha"], ["GET", "/admin", "Bearer beta"]
  ];
  const requests = [];
  for (let i = 0; i < 64; i++) {
    const input = inputs[i % inputs.length];
    requests.push({ method: input[0], path: input[1], headers: { authorization: input[2] }, body: "" });
  }
  return summarize(requests, handle);
}

function respond(status, value) {
  return { status, headers: { "content-type": "application/json" }, body: JSON.stringify(value) };
}

function summarize(requests, handle) {
  const responses = [];
  for (const request of requests) responses.push(handle(request));
  return { requests: requests.length, responses };
}

print(JSON.stringify(run()));
