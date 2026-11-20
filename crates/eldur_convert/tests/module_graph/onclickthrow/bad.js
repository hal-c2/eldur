import { log } from "./log.js";
const btn = document.getElementById("go");
function fail() { throw new Error("bad"); }
log("bad runs");
fail();
btn.onclick = () => log("bad onclick");
