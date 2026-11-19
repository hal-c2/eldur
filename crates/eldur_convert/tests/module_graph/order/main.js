import { log } from "./log.js";
import { a } from "./a.js";
import { b } from "./lib/b.js";
import { ping } from "./cycle1.js";
log("main " + a + " " + b + " " + ping());
export function later() { return import("./never.js"); }
const lazy = import("./lazy.js");
log("after import call");
Promise.resolve().then(() => log("microtask"));
lazy
  .then((m) => {
    log("lazy namespace " + m.word + " " + m.shared());
    return import("./lazy.js").then((again) => log("same namespace " + (again === m)));
  })
  .then(() => import("./throws.js"))
  .then(() => log("throws resolved"), (e) => log("throws rejected " + e.message))
  .then(() => import("./throws.js"))
  .then(null, (e) => log("throws again " + e.message))
  .then(() => import("./missing.js"))
  .then(null, (e) => log("missing " + e.name + " " + e.message.startsWith("Failed to fetch dynamically imported module")))
  .then(() => import(`./lazydep.js`))
  .then((m) => log("template " + m.only))
  .then(() => import("bare-package"))
  .then(null, (e) => log("bare " + e.name))
  .then(() => { document.getElementById("out").textContent = "done"; log("done"); });
