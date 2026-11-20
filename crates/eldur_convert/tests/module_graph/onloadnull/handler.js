import { log } from "./log.js";
function fail(what) { throw new Error(what); }
log("handler runs");
fail("handler");
window.onload = () => {
  log("handler onload");
  document.getElementById("out").textContent = "handler";
};
