import { log } from "./log.js";
import { ping } from "./cycle1.js";
log("cycle2 " + typeof ping);
export function pong() { return "pong"; }
