// CPU-only request/response records; no Fetch, network, host clock, or external packages.
function handle(request) {
  if (request.method !== "POST") return respond(405, { error: "method" });
  let order;
  try {
    order = JSON.parse(request.body);
  } catch (error) {
    return respond(400, { error: "json" });
  }
  if (!order || typeof order.customer !== "string" || !order.customer.trim() || !Array.isArray(order.items) || !order.items.length) {
    return respond(422, { error: "order" });
  }
  const prices = { tea: 350, cake: 450 };
  for (const item of order.items) {
    if (!item || !Object.hasOwn(prices, item.sku) || !Number.isInteger(item.quantity) || item.quantity < 1 || item.quantity > 10) {
      return respond(422, { error: "item" });
    }
  }
  const lines = order.items.map(item => ({ sku: item.sku, quantity: item.quantity, cents: prices[item.sku] * item.quantity }));
  const total = lines.reduce((sum, line) => sum + line.cents, 0);
  return respond(201, { customer: order.customer.trim(), lines, total });
}

function run() {
  const bodies = [
    '{"customer":" Zoë ","items":[{"sku":"tea","quantity":2},{"sku":"cake","quantity":1}]}',
    '{"customer":"& <苑>","items":[{"sku":"tea","quantity":1}]}',
    '{"customer":" ","items":[{"sku":"tea","quantity":1}]}',
    '{"customer":"Ada","items":[{"sku":"tea","quantity":0}]}',
    '{broken',
    '{}',
    'null',
    '{"customer":"Ada","items":[{"sku":"toString","quantity":1}]}'
  ];
  const requests = [];
  for (let i = 0; i < 64; i++) {
    let method = "POST";
    if (i % bodies.length === 5) method = "GET";
    requests.push({ method, path: "/orders", headers: {}, body: bodies[i % bodies.length] });
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
