import { log } from "./log.js";
const btn = document.getElementById("go");
btn.addEventListener("click", () => {
  log("good heard");
  document.getElementById("out").textContent = "clicked";
});
