// Builds the computed.mjs cases for conditional.rs: one page per (test, combination),
// with the combination's classes written statically. See README.md.
import fs from "node:fs";
const P = ["color","opacity","display","font-size","font-weight","padding-top","padding-left","margin-top","margin-right","margin-bottom","margin-left","padding-right","padding-bottom"];
const cases = [];
function add(name, css, body, select, props = P) { cases.push({ name, html: `<style>${css}</style>${body}`, select, props }); }
// 1 descendant
const t1css = ".panel{display:none;color:rgb(10,20,30)} .open .panel{display:block;color:rgb(200,0,0)}";
for (const open of [0,1]) for (const sel of ["#panel","#label"]) add(`desc open=${open} ${sel}`, t1css, `<div id=menu${open?" class=open":""}><div id=panel class=panel><span id=label>x</span></div></div>`, sel);
// 2 sibling
const t2css = ".on + p{color:rgb(255,0,0)} .on ~ p{font-size:20px}";
for (const on of [0,1]) for (const sel of ["#b","#c"]) add(`sib on=${on} ${sel}`, t2css, `<div><span id=a class="tab${on?" on":""}">a</span><p id=b>b</p><p id=c>c</p></div>`, sel);
// 3 hover + class (:hover written as .hov for the static page)
const t3css = (h) => `.item{opacity:1} .item${h}{opacity:0.9} .item.done{opacity:0.6} .item.done${h}{color:rgb(0,128,0)}`;
for (const done of [0,1]) for (const hov of [0,1]) add(`hover done=${done} hover=${hov}`, t3css(".hov"), `<ul><li id=i class="item${done?" done":""}${hov?" hov":""}">x</li></ul>`, "#i");
// 4 :not
for (const read of [0,1]) add(`not read=${read}`, ".msg:not(.read){font-weight:700}", `<p id=p class="msg${read?" read":""}">x</p>`, "#p");
// 5 present and removed
for (const warn of [0,1]) add(`present warn=${warn}`, ".note{color:rgb(0,0,255)} .warn{color:rgb(255,165,0)}", `<p id=p class="note${warn?" warn":""}">x</p>`, "#p");
// 6 specificity and order
const t6css = ".b.b{color:rgb(0,128,0)} .a{color:rgb(255,0,0)} .b{color:rgb(0,0,255);margin-top:3px} .a{margin-top:5px}";
for (const a of [0,1]) for (const b of [0,1]) add(`spec a=${a} b=${b}`, t6css, `<p id=p class="${a?"a ":""}${b?"b":""}">x</p>`, "#p");
// 7 inherited
const t7css = ".big{font-size:20px;color:rgb(1,2,3)} p{padding-left:1em;margin-top:0}";
for (const big of [0,1]) for (const sel of ["#t","#s"]) add(`inherit big=${big} ${sel}`, t7css, `<div id=box${big?" class=big":""}><p id=t><span id=s>x</span></p></div>`, sel);
// 8 cap: seven classes
const t8css = ".c1{margin-top:1px} .c2{margin-right:2px} .c3{margin-bottom:3px} .c4{margin-left:4px} .c5{padding-top:5px} .c6{padding-right:6px} .c7{padding-bottom:7px}";
for (const set of [[], [1,2,3,4,5,6], [2,5], [1,2,3,4,5,6,7]]) add(`cap ${set.join(",")}`, t8css, `<p id=p class="${set.map(i=>"c"+i).join(" ")}">x</p>`, "#p");
// 9 :checked as a state condition (the `checked` attribute for the static page)
for (const on of [0,1]) add(`checked on=${on}`, "input:checked + label{color:rgb(0,128,0)}", `<input id=cb type=checkbox${on?" checked":""}><label id=lb>l</label>`, "#lb");
// 10 var() under a toggled class
for (const dark of [0,1]) add(`var dark=${dark}`, ".dark{--fg:rgb(9,9,9)} p{color:var(--fg, rgb(1,1,1))}", `<div id=box${dark?" class=dark":""}><p id=t>x</p></div>`, "#t");
fs.writeFileSync(process.argv[2], JSON.stringify(cases, null, 1));
