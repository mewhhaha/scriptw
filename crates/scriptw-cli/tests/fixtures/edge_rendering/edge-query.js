// Percent decoding and application query policy over plain path records, without URL/Fetch hosts.
function decode(value) {
  return decodeURIComponent(value.split("+").join(" "));
}

function handle(request) {
  const question = request.path.indexOf("?");
  let q = "";
  let limit = 10;
  const tags = [];
  if (question !== -1) {
    try {
      for (const field of request.path.slice(question + 1).split("&")) {
        const equals = field.indexOf("=");
        let key = field;
        let value = "";
        if (equals !== -1) {
          key = field.slice(0, equals);
          value = field.slice(equals + 1);
        }
        key = decode(key);
        value = decode(value);
        if (key === "q") q = value.trim().toLowerCase();
        if (key === "tag") tags.push(value);
        if (key === "limit") {
          const number = Number(value);
          if (Number.isFinite(number)) limit = Math.max(1, Math.min(20, Math.floor(number)));
        }
      }
    } catch (error) {
      return respond(400, { error: "encoding" });
    }
  }
  tags.sort();
  return respond(200, { q, limit, tags });
}

function run() {
  const paths = [
    "/search?q=green+tea&limit=2&tag=hot&tag=leaf",
    "/search?tag=%E8%8C%B6&q=CAF%C3%89&limit=99",
    "/search?q=%3Ctea%3E&limit=-2&tag=a%2Bb",
    "/search?empty=&q=a%26b&limit=3&tag=x&tag=y",
    "/search?q=%E0%A4%A", "/search?limit=abc&q=two%20words",
    "/search?q=one&q=two&tag=%26%3D&limit=0", "/search"
  ];
  const requests = [];
  for (let i = 0; i < 64; i++) requests.push({ method: "GET", path: paths[i % paths.length], headers: {}, body: "" });
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
