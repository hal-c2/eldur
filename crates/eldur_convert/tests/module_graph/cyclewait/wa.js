import { log } from "./log.js";
import { b } from "./wb.js";
log("wa runs " + b);
import("./wb.js")
  .then(() => log("wb resolved in wa"), (e) => log("wb rejected in wa " + e.message));
throw new Error("wa");
