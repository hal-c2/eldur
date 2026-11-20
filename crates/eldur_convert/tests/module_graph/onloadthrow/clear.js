import { log } from "./log.js";
function fail(what) { throw new Error(what); }
log("clear runs");
fail("clear");
window.onload = null;
