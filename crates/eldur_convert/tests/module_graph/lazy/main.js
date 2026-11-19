import { log } from "./log.js";
import * as live from "./counter.js";
// Named before the modules that import them back, so each cycle is entered
// from its far side when the near side is imported first.
if (false) { import("./ca.js"); import("./xa.js"); }
log("live " + live.count);
live.bump();
log("live after bump " + live.count);
import("./cb.js")
  .then((m) => log("cb " + m.b))
  .then(() => import("./ca.js"))
  .then((m) => log("ca " + m.a))
  .then(() => import("./xb.js"))
  .then(() => log("xb resolved"), (e) => log("xb rejected " + e.message))
  .then(() => import("./xa.js"))
  .then(() => log("xa resolved"), (e) => log("xa rejected " + e.message))
  .then(() => import("./broken.js"))
  .then(() => log("broken resolved"), (e) => log("broken " + e.name + " " + e.message.startsWith("Failed to fetch dynamically imported module")))
  .then(() => import("./shape.js"))
  .then((m) => log("shape " + new m.Square(3).area() + " " + new m.default(3).perimeter()))
  .then(() => import("./ta.js"))
  .then((m) => {
    log("ta " + m.K + " " + m.n + " " + m.default + " " + m.readLate() + " " + typeof m.Box);
    try { m.assignK(); log("assignK ok"); } catch (e) { log("assignK " + e.name); }
    try { m.destructureK(); log("destructureK ok"); } catch (e) { log("destructureK " + e.name); }
    m.setN(5);
    log("ta n " + m.n);
    m.loopN();
    m.destructure(m.n + 1);
    log("ta n after loop and destructure " + m.n);
  })
  .then(() => import("./listens.js"))
  .then(() => new Promise((resolve) => setTimeout(resolve, 50)))
  .then(() => { document.getElementById("out").textContent = "done"; log("done"); });
