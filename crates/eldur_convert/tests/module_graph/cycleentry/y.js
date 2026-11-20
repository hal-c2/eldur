import { log } from "./log.js";
import { x } from "./x.js";
export const y = "y";
log("y runs " + x);
import("./x.js")
  .then((ns) => log("x resolved in y " + ns.x), (e) => log("x rejected in y " + e.message));
throw new Error("y");
