import { log } from "./log.js";
import { pong } from "./cycle2.js";
log("cycle1");
export function ping() { return "ping" + pong(); }
