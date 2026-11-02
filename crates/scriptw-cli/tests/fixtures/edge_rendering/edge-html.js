// Application HTML serialization over request/response records; no DOM or UI framework.
function escapeHtml(value) {
  return String(value).split("&").join("&amp;").split("<").join("&lt;").split(">").join("&gt;").split('"').join("&quot;").split("'").join("&#39;");
}

function handle(request) {
  const page = JSON.parse(request.body);
  const html = ["<article><h1>", escapeHtml(page.title), "</h1><table><tbody>"];
  for (const row of page.rows) {
    html.push('<tr data-id="', escapeHtml(row.id), '"><td>', escapeHtml(row.name), "</td><td>", escapeHtml(row.quantity), "</td></tr>");
  }
  html.push("</tbody></table></article>");
  return { status: 200, headers: { "content-type": "text/html; charset=utf-8" }, body: html.join("") };
}

function run() {
  const titles = ["Today's <menu>", "茶 & café", '"Seasonal"', "Plain"];
  const names = ["Tea & cake", "<b>Soup</b>", 'Oat "milk"', "Crème d'été"];
  const requests = [];
  for (let i = 0; i < 32; i++) {
    const rows = [];
    for (let j = 0; j < names.length; j++) rows.push({ id: "row-" + i + "-" + j + "'", name: names[j], quantity: (i + j) % 3 + 1 });
    requests.push({ method: "GET", path: "/menu", headers: {}, body: JSON.stringify({ title: titles[i % titles.length], rows }) });
  }
  return summarize(requests, handle);
}

function summarize(requests, handle) {
  const responses = [];
  for (const request of requests) responses.push(handle(request));
  return { requests: requests.length, responses };
}

print(JSON.stringify(run()));
