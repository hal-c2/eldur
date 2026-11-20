import { log } from "./log.js";
window.onload = () => {
  log("late onload");
  document.getElementById("out").textContent = "late";
};
log("late runs");
