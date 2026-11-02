// Explicit request/response records: route dispatch only, without a Web Fetch implementation.
function handle(request) {
  let path = request.path.split("?")[0];
  if (path.length > 1 && path.endsWith("/")) path = path.slice(0, -1);
  if (path === "/health") {
    if (request.method === "HEAD") return { status: 200, headers: { "content-type": "application/json" }, body: "" };
    if (request.method !== "GET") return respond(405, { allow: "GET, HEAD" });
    return respond(200, { ok: true });
  }
  if (path === "/echo") {
    if (request.method !== "POST") return respond(405, { allow: "POST" });
    return respond(201, { echo: request.body });
  }
  const segments = path.split("/");
  if (segments.length === 3 && segments[1] === "products") {
    if (request.method !== "GET") return respond(405, { allow: "GET" });
    const prices = { tea: 350, cake: 450 };
    const sku = segments[2];
    if (Object.hasOwn(prices, sku)) return respond(200, { sku, cents: prices[sku] });
  }
  return respond(404, { error: "not found", path });
}

function run() {
  const routes = [
    ["GET", "/health"], ["GET", "/products/tea?currency=SEK"],
    ["POST", "/products/tea"], ["GET", "/products/missing"],
    ["GET", "/products/cake/"], ["POST", "/echo"],
    ["GET", "/missing"], ["HEAD", "/health"]
  ];
  const requests = [];
  for (let i = 0; i < 64; i++) {
    const route = routes[i % routes.length];
    requests.push({ method: route[0], path: route[1], headers: {}, body: "茶 & <cake> " + i });
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
