// GraphQL-style result shaping, without a GraphQL parser or host request implementation.
function run() {
  const members = [{ id: "u1", name: "Ada" }, { id: "u2", name: "Zoë 茶" }];
  const projects = [
    { id: "p1", title: "Tea & <cake>", tasks: [
      { id: "t1", title: "Buy tea", done: false, private: false, assignee: "u2" },
      { id: "t2", title: 'Write "menu"', done: true, private: false, assignee: "u1" },
      { id: "t3", title: "Budget", done: false, private: true, assignee: "u1" }
    ] },
    { id: "p2", title: "Café", tasks: [
      { id: "t4", title: "Open", done: false, private: false, assignee: "u1" },
      { id: "t5", title: "Bake", done: false, private: false, assignee: "u2" }
    ] }
  ];
  function handle(request) {
    let query;
    try {
      query = JSON.parse(request.body);
    } catch (error) {
      return respond(400, { errors: ["json"] });
    }
    if (query.operation !== "Project") return respond(400, { errors: ["operation"] });
    const project = projects.find(project => project.id === query.id);
    if (!project) return respond(404, { data: { project: null } });
    const visible = project.tasks.filter(task => (query.includeDone || !task.done) && (request.headers.authorization === "owner" || !task.private));
    const tasks = visible.slice(0, query.limit).map(task => ({
      id: task.id, title: task.title, done: task.done,
      assignee: members.find(member => member.id === task.assignee)
    }));
    return respond(200, { data: { project: {
      id: project.id, title: project.title, members, tasks,
      pageInfo: { total: visible.length, returned: tasks.length, hasMore: tasks.length < visible.length }
    } } });
  }
  const inputs = [
    ['{"operation":"Project","id":"p1","includeDone":false,"limit":10}', "owner"],
    ['{"operation":"Project","id":"p2","includeDone":false,"limit":10}', "owner"],
    ['{"operation":"Project","id":"p1","includeDone":true,"limit":10}', "owner"],
    ['{"operation":"Project","id":"missing","limit":10}', "owner"],
    ['{"operation":"Mutation"}', "owner"], ['{broken', "owner"],
    ['{"operation":"Project","id":"p2","includeDone":false,"limit":1}', "owner"],
    ['{"operation":"Project","id":"p1","includeDone":true,"limit":10}', "guest"]
  ];
  const requests = [];
  for (let i = 0; i < 48; i++) {
    const input = inputs[i % inputs.length];
    requests.push({ method: "POST", path: "/graphql", headers: { authorization: input[1] }, body: input[0] });
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
