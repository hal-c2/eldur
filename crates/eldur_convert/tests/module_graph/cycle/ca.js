import { log } from "./log.js";
import { b } from "./cb.js";
log("ca runs " + b);
throw new Error("ca");
