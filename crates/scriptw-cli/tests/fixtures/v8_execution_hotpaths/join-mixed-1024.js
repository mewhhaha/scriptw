(function workload(){
var a=[];for(var i=0;i<1024;i++)a.push(i%4===0?null:i%4===1?undefined:i%4===2?"xy":"z");print(a.join(",").length);
})();
