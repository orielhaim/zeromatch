import pm from "picomatch";
import { Bench } from "tinybench";
import { Glob, isMatch } from "../index.js";

const pattern = "**/needle.{js,ts,tsx,jsx,mdx}";
const path = "some/a/bigger/path/to/the/crazy/needle.ts";

const pmFn = pm(pattern);
const zm = new Glob(pattern);

const b = new Bench({ time: 2000 });

b.add("picomatch (cached)", () => {
  pmFn(path);
});
b.add("zeromatch (cached)", () => {
  zm.test(path);
});
b.add("picomatch (one-shot)", () => {
  pm.isMatch(path, pattern);
});
b.add("zeromatch (one-shot)", () => {
  isMatch(path, pattern);
});

const paths = Array.from({ length: 1000 }, () => path);
b.add("picomatch (cached x1000)", () => {
  for (let i = 0; i < paths.length; i++) pmFn(paths[i]);
});
b.add("zeromatch (testMany x1000)", () => {
  zm.testMany(paths);
});

await b.run();
console.table(b.table());
