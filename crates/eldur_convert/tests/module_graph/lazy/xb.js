import { log } from "./log.js";
import { xa } from "./xa.js";
log("xb runs after " + xa);
throw new Error("xb boom");
