import { log } from "./log.js";
const late = () => {
  log("module load");
};
window.addEventListener("load", late);
window.addEventListener("load", late);
log("module runs");
