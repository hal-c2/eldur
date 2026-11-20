import { log } from "./log.js";
import { b } from "./xb.js";
import { c } from "./xc.js";
log("xa runs " + b + c);
throw new Error("xa");
