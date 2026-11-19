import { label } from "./helper.js";
document.getElementById("a").textContent = label(2);
import("./chunk.js").then((m) => { document.getElementById("b").textContent = m.word; },
                          (e) => { document.getElementById("b").textContent = "rejected"; });
