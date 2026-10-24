(function workload(){
var a="abcd",b="efgh",s=0;for(var i=0;i<4096;i++){a=i%2?"abcd":"efgh";b=i%2?"efgh":"abcd";s+=(a+b).length;}print(s);
})();
