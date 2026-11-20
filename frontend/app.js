const source = document.querySelector("#source");
const nums = document.querySelector("#line-numbers");
let activeFile = "text_io.ufp", selectedNode = "tokenize", speed = 1, timer, runIndex = 0;
const nodes = ["input", "tokenize", "classify", "output"];
let connections = [["input","tokenize"],["tokenize","classify"],["classify","output"]];
let ufpApiAvailable = false;
const names = {input:"Input", tokenize:"Tokenize", classify:"Classify", output:"Output"};
function setText(text) { source.value = text; updateLines(); }
function updateLines() { nums.textContent = Array.from({length: source.value.split("\n").length}, (_,i)=>i+1).join("\n"); }
function save() { if (activeFile.endsWith(".ufp")) localStorage.setItem(`ufp:${activeFile}`,source.value); updateLines(); }
async function loadFile(file) {
  activeFile=file; document.querySelectorAll(".file").forEach(b=>b.classList.toggle("active",b.dataset.file===file));
  document.querySelector("#crumb-file").textContent=file; document.querySelector("#text-title").textContent=file;
  const inert=file.endsWith(".txt");
  document.querySelector("#active-file-icon").textContent=inert?"≡":"◇";
  document.querySelector("#active-file-icon").className=`file-icon ${inert?"amber":"purple"}`;
  document.querySelector("#file-kind").textContent=inert?"INERT TXT · NOT EXECUTABLE":"UFP SOURCE · PROTOTYPE";
  source.readOnly=inert;
  source.classList.toggle("inert-source",inert);
  document.querySelector("#validate").disabled=inert;
  document.querySelector("#run").disabled=inert;
  source.value="Loading repository example…"; updateLines();
  const examplePath=file==="layer_000.ufp"?"layers/layer_000.ufp":file;
  try {
    const response=await fetch(`./examples/${examplePath}`);
    if(!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    const actual=await response.text();
    setText(inert?actual:(localStorage.getItem(`ufp:${file}`) ?? actual));
    document.querySelector("#validation-label").textContent=inert?"Inert text · not compiled or executable":"Repository example · edits saved locally";
    document.querySelector("#validation-label").style.color="";
  } catch(error) {
    setText(`Could not load bundled example: ${error.message}`);
    document.querySelector("#validation-label").textContent="Example bundle unavailable";
    document.querySelector("#validation-label").style.color="#e4a078";
  }
}
async function checkBackend() {
  const status=document.querySelector("#backend-status"), dot=document.querySelector("#health-dot");
  try {
    const health=await fetch("/api/health").then(r=>{if(!r.ok)throw Error(`HTTP ${r.status}`);return r.json()});
    status.innerHTML=`Backend connected<small>${health.lab} · ${health.version}</small>`;
    dot.style.background="#67d6a0";
    const graph=await fetch("/api/graph/demo").then(r=>{if(!r.ok)throw Error(`HTTP ${r.status}`);return r.json()});
    const validation=await fetch("/api/graph/validate",{method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(graph)}).then(r=>{if(!r.ok)throw Error(`HTTP ${r.status}`);return r.json()});
    const messages=[...(validation.errors||[]).map(x=>`ERROR: ${x}`),...(validation.warnings||[]).map(x=>`Warning: ${x}`)];
    document.querySelector("#wire-status").textContent=`Backend demo graph: ${validation.ok?"valid":"invalid"} · ${validation.order?.length||0} ordered nodes`;
    if(messages.length) addLog(`<span class="log-time">backend diagnostics</span>${messages.map(x=>`<span class="log-node">${x}</span>`).join("")}`);
    else addLog(`<span class="log-time">backend</span><span class="log-ok">Demo graph validated by backend</span><span>Not validation of the open UFP file</span>`);
  } catch(error) {
    status.innerHTML=`Backend unavailable<small>${error.message}</small>`;
    dot.style.background="#e4a078";
    document.querySelector("#wire-status").textContent="Backend graph validation unavailable";
  }
}
async function ufpRequest(action, body) {
  const response = await fetch(`/api/ufp/${action}`, {method:"POST",headers:{"content-type":"application/json"},body:JSON.stringify(body)});
  let data;
  try { data = await response.json(); } catch { data = {message: await response.text().catch(()=>response.statusText)}; }
  if (!response.ok) { const error = new Error(data.message || data.error || `HTTP ${response.status}`); error.data=data; throw error; }
  ufpApiAvailable = true;
  return data;
}
function printDiagnostics(data, label) {
  const list = data?.diagnostics || data?.errors || [];
  const items = Array.isArray(list) ? list : [list];
  const terminal = document.querySelector("#terminal");
  if (!items.length) { addLog(`<span class="log-time">${label}</span><span class="log-ok">${data?.ok === false ? "Failed" : "No diagnostics"}</span>`); return; }
  for (const item of items) {
    const row = document.createElement("div"); row.className="log-line";
    const severity = String(item.severity || item.level || (data?.ok === false ? "error" : "warning")).toUpperCase();
    const loc = item.line ? `:${item.line}${item.column ? `:${item.column}` : ""}` : "";
    row.textContent = `${label} ${severity}${item.code ? ` ${item.code}` : ""}${loc}: ${item.message || item}`;
    terminal.append(row);
  }
}
function addTextLog(label, text, success=false) {
  const row=document.createElement("div"); row.className="log-line";
  const time=document.createElement("span"); time.className="log-time"; time.textContent=label;
  const message=document.createElement("span"); message.className=success?"log-ok":""; message.textContent=text;
  row.append(time,message); document.querySelector("#terminal").append(row);
}
function selectNode(id) {
  selectedNode=id; document.querySelectorAll(".node").forEach(n=>n.classList.toggle("selected",n.dataset.node===id));
  document.querySelector("#inspect-name").textContent=names[id]; document.querySelector("#inspect-type").textContent=id==="tokenize"?"Text operation":id==="classify"?"Model operation":id==="input"?"Plan input":"Plan output";
  document.querySelector("#node-name").value=id==="tokenize"?"split_words":id==="classify"?"sentiment":id;
}
function validate() {
  const text=source.value, open=(text.match(/\{/g)||[]).length, close=(text.match(/\}/g)||[]).length;
  const hasPlan=/\b(?:input|output|procedure|portal|node|matrix|binary)\b/.test(text), hasInput=/\binput\b/.test(text), hasOutput=/\boutput\b/.test(text);
  const wiringOk=connections.length===3;
  const ok=hasPlan&&hasInput&&hasOutput&&open===close&&wiringOk;
  document.querySelector("#wire-status").textContent=ok?"Wiring looks connected":!wiringOk?"Wiring incomplete · connect all ports":"Check plan, input/output and braces";
  document.querySelector("#validation-label").textContent=ok?"Local checks passed · API check pending":"Local check needs attention";
  document.querySelector("#validation-label").style.color=ok?"#67d6a0":"#e4a078";
  return ok;
}
function addLog(html){const terminal=document.querySelector("#terminal");terminal.insertAdjacentHTML("beforeend",`<div class="log-line">${html}</div>`);terminal.scrollTop=terminal.scrollHeight}
function runStep() {
  if(runIndex>=nodes.length){clearInterval(timer);timer=null;document.querySelector("#session-state").textContent="COMPLETE";document.querySelector("#session-state").classList.remove("active");addLog('<span class="log-time">done</span><span class="log-ok">Demo session complete · no model was run</span>');return}
  const id=nodes[runIndex++]; document.querySelectorAll(".node").forEach(n=>n.classList.toggle("running",n.dataset.node===id));
  addLog(`<span class="log-time">step ${runIndex}</span><span class="log-node">${names[id]}</span><span>local demo step</span><span class="log-ok">✓</span>`);
  if(runIndex===nodes.length) document.querySelector(".node[data-node='output']").classList.add("running");
}
document.querySelectorAll(".file").forEach(b=>b.addEventListener("click",()=>loadFile(b.dataset.file)));
source.addEventListener("input",save);
source.addEventListener("keydown",e=>{if(e.key==="Tab"){e.preventDefault();const a=source.selectionStart,b=source.selectionEnd;source.setRangeText("  ",a,b,"end");save()}});
document.querySelectorAll(".tab").forEach(b=>b.addEventListener("click",()=>{document.querySelectorAll(".tab").forEach(t=>t.classList.toggle("active",t===b));document.querySelector("#editor").className=`editor view-${b.dataset.view}`}));
 document.querySelector("#validate").addEventListener("click",async()=>{validate();const sourceText=source.value;document.querySelector("#terminal").innerHTML="";try{const parsed=await ufpRequest("parse",{source:sourceText});printDiagnostics(parsed,"parse");if(parsed.ok===false)return;const checked=await ufpRequest("check",{source:sourceText});printDiagnostics(checked,"check");const ok=checked.ok!==false&&!(checked.errors||[]).length&&!(checked.diagnostics||[]).some(d=>/error/i.test(d.severity||d.level||""));document.querySelector("#validation-label").textContent=ok?"UFP parsed and checked by backend":"Backend diagnostics reported issues";document.querySelector("#wire-status").textContent=ok?"UFP source accepted by backend":"UFP source needs attention";}catch(error){ufpApiAvailable=false;if(error.data)printDiagnostics(error.data,"backend");const ok=validate();addTextLog("fallback",`${ok?"Local canvas checks passed":"Local canvas checks found issues"} · UFP API unavailable: ${error.message}`,ok)}});
 document.querySelector("#run").addEventListener("click",async()=>{if(timer){clearInterval(timer);timer=null;document.querySelector("#session-state").textContent="PAUSED";document.querySelector("#session-state").classList.remove("active");return}clearInterval(timer);timer=null;document.querySelectorAll(".node").forEach(n=>n.classList.remove("running"));document.querySelector("#terminal").innerHTML="";document.querySelector("#session-state").textContent="RUNNING";document.querySelector("#session-state").classList.add("active");try{const result=await ufpRequest("run",{source:source.value,inputs:{prompt:document.querySelector("#session-input").value}});printDiagnostics(result,"run");for(const [name,value] of Object.entries(result.outputs||{}))addTextLog("output",`${name}: ${String(value)}`,true);for(const event of result.events||[])addTextLog("event",typeof event==="string"?event:JSON.stringify(event));if(result.output!==undefined)addTextLog("output",String(result.output),true);document.querySelector("#session-state").textContent=result.ok===false?"ERROR":"COMPLETE";document.querySelector("#session-state").classList.remove("active");}catch(error){ufpApiAvailable=false;document.querySelector("#session-state").classList.remove("active");if(error.data){printDiagnostics(error.data,"run");document.querySelector("#session-state").textContent="ERROR";return}document.querySelector("#session-state").textContent="DEMO";addTextLog("fallback",`UFP run API unavailable · ${error.message}`);addTextLog("fallback","Showing local canvas mock; no UFP runtime was called.");runIndex=0;runStep();timer=setInterval(runStep,1000/speed)}});
document.querySelectorAll(".speed button").forEach(b=>b.addEventListener("click",()=>{speed=Number(b.dataset.speed);document.querySelectorAll(".speed button").forEach(x=>x.classList.toggle("selected",x===b));if(timer){clearInterval(timer);timer=setInterval(runStep,1000/speed)}}));
document.querySelector("#clear-log").addEventListener("click",()=>document.querySelector("#terminal").innerHTML='<div class="log-muted">Session log cleared.</div>');
document.querySelectorAll(".node").forEach(n=>{
  n.addEventListener("click",()=>selectNode(n.dataset.node));
  n.addEventListener("dragstart",e=>{e.dataTransfer.setData("text/plain",n.dataset.node);n.classList.add("dragging")});
  n.addEventListener("dragend",()=>n.classList.remove("dragging"));
});
document.querySelectorAll(".port").forEach(port=>{
  port.draggable=true;
  port.addEventListener("dragstart",e=>{e.stopPropagation();e.dataTransfer.setData("text/ufp-port",JSON.stringify({node:port.closest(".node").dataset.node,direction:port.classList.contains("output")?"out":"in"}))});
  port.addEventListener("dragover",e=>{if(e.dataTransfer.types.includes("text/ufp-port"))e.preventDefault()});
  port.addEventListener("drop",e=>{e.preventDefault();e.stopPropagation();try{const from=JSON.parse(e.dataTransfer.getData("text/ufp-port")),to={node:port.closest(".node").dataset.node,direction:port.classList.contains("output")?"out":"in"};if(from.direction==="out"&&to.direction==="in"&&from.node!==to.node){connections=connections.filter(c=>c[0]!==from.node&&c[1]!==to.node);connections.push([from.node,to.node]);drawWires();validate();addLog(`<span class="log-time">wire</span><span class="log-ok">${names[from.node]} → ${names[to.node]} connected locally</span>`)}}catch{}} )
});
const canvas=document.querySelector("#canvas");
canvas.addEventListener("dragover",e=>e.preventDefault());
canvas.addEventListener("drop",e=>{e.preventDefault();const id=e.dataTransfer.getData("text/plain"),node=document.querySelector(`.node[data-node="${id}"]`);if(!node)return;const r=canvas.getBoundingClientRect();node.style.left=`${Math.max(0,(e.clientX-r.left)/r.width*100-7)}%`;node.style.top=`${Math.max(5,(e.clientY-r.top)/r.height*100-5)}%`;drawWires()});
function drawWires(){const svg=document.querySelector("#wires"),rect=canvas.getBoundingClientRect();svg.setAttribute("viewBox",`0 0 ${rect.width} ${rect.height}`);svg.innerHTML="";connections.forEach(([a,b])=>{const x=document.querySelector(`.node[data-node="${a}"]`).getBoundingClientRect(),y=document.querySelector(`.node[data-node="${b}"]`).getBoundingClientRect();let x1=x.right-rect.left,y1=x.top-rect.top+x.height*.65,x2=y.left-rect.left,y2=y.top-rect.top+y.height*.65;if(x2<x1){x1=x.left-rect.left+x.width/2;y1=x.bottom-rect.top;x2=y.left-rect.left+y.width/2;y2=y.top-rect.top}const dx=Math.abs(x2-x1)*.45,path=document.createElementNS("http://www.w3.org/2000/svg","path");path.setAttribute("d",`M ${x1} ${y1} C ${x1+dx} ${y1}, ${x2-dx} ${y2}, ${x2} ${y2}`);path.setAttribute("class","wire");svg.append(path)})}
document.querySelector("#node-name").addEventListener("input",e=>{if(selectedNode==="tokenize")document.querySelector(".node[data-node='tokenize'] .node-body b").textContent=e.target.value});
document.querySelector("#lowercase").addEventListener("change",e=>document.querySelector(".node[data-node='tokenize'] .node-body small").textContent=`lowercase · ${e.target.checked}`);
document.querySelector("#portal-open").addEventListener("click",()=>{loadFile("layer_000.ufp");document.querySelector(".file[data-file='layer_000.ufp']").scrollIntoView?.({block:"nearest"});});
 document.querySelector("#new-file").addEventListener("click",()=>{const name=prompt("New local UFP filename","untitled.ufp");if(!name)return;const file=name.endsWith(".ufp")?name:`${name}.ufp`;const nav=document.querySelector(".file-list"),b=document.createElement("button");b.className="file";b.dataset.file=file;b.innerHTML=`<span class="file-icon purple">◇</span>${file}<span class="file-type">UFP</span>`;b.onclick=()=>loadFile(file);nav.insertBefore(b,nav.querySelector(".inert-group"));localStorage.setItem(`ufp:${file}`,'input prompt = "hello";\noutput reply;\npipe prompt.text -> reply.text;');loadFile(file)});
document.querySelectorAll(".node").forEach(n=>n.addEventListener("dragend",drawWires));
 window.addEventListener("resize",drawWires);loadFile(activeFile);checkBackend();requestAnimationFrame(drawWires);
