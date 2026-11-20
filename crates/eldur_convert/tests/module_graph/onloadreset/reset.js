import { log } from "./log.js";
window.onload = null;
window.onload = () => {
  log("reset onload");
  document.getElementById("out").textContent = "reset";
};
log("reset runs");
