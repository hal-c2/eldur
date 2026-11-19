import { log } from "./log.js";
log("last runs");
import("./boom.js")
  .then(() => log("late resolved"), (e) => log("late rejected " + e.message))
  .then(() => import("./after.js"))
  .then(() => log("after resolved"), (e) => log("after rejected " + e.message))
  .then(() => { document.getElementById("out").textContent = "done"; log("done"); });
