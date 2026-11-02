globalThis.applicationRun = async function (mode) {
  let status=0;
  let bytes=0;
  let sum=0;
  for (let i=0; i<32; i++) {
    let response;
    if (mode === 'url') {
      const request=new Request('https://edge.invalid/items/'+i+'?q=caf%C3%A9&tag=a&tag=b');
      const url=new URL(request.url);
      response=Response.json({id:Number(url.pathname.split('/')[2]), query:url.searchParams.get('q'), tags:url.searchParams.getAll('tag')});
    } else if (mode === 'json') {
      const request=new Request('https://edge.invalid/orders', {method:'POST', headers:{'content-type':'application/json'},
        body:JSON.stringify({id:i, quantity:i%4+1, price:125})});
      const order=await request.json();
      response=Response.json({id:order.id, total:order.quantity*order.price}, {status:201,headers:{'x-policy':'orders-v1'}});
    } else {
      throw new Error('Unknown Web API operation');
    }
    const body=await response.text();
    status+=response.status;
    bytes+=body.length;
    for (let j=0;j<body.length;j++) sum=(sum+body.charCodeAt(j))>>>0;
  }
  return {mode, requests:32, status, bytes, sum};
};
