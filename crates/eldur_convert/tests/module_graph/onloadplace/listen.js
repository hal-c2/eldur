import { log } from "./log.js";
window.addEventListener("load", () => {
  log("listener");
  document.getElementById("out").textContent = "listener";
});
log("listen runs");
