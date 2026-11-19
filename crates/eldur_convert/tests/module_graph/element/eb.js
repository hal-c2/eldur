import { log } from "./log.js";
import { read } from "./ea.js";
let early;
try {
  early = read();
} catch (e) {
  early = e.name;
}
log("eb reads ea's element " + early);
export { early };
