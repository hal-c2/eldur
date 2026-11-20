import { log } from "./log.js";
let later = () => {
  log("module load a");
};
window.addEventListener("load", later);
later = () => {
  log("module load b");
};
log("module runs");
