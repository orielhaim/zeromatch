# zeromatch

A fast, picomatch-compatible glob matcher for Node.js, written in Rust.

```bash
bun add zeromatch
```

## Why

Glob matching is on the hot path of nearly every JS build tool, bundler, and file watcher. `picomatch` is excellent - it compiles patterns to regular expressions and leans on V8's regex engine, which is hard to beat once a pattern is cached. But for *uncached* matching (compiling a pattern then matching once), or for batched workloads where you need to test many paths against the same pattern, there's room to do better.

zeromatch is a clean-room reimplementation: a small bytecode VM with specialized fast paths for the patterns you actually use. It's API-compatible with picomatch's common surface, so swapping it in is straightforward.

## When to use it

zeromatch is a good fit when:

- **You match the same pattern against many paths.** This is the case picomatch's `pmFn` covers; zeromatch covers it too, and the batched `testMany` form is significantly faster than calling a JS matcher in a loop.
- **You compile a pattern on each call.** zeromatch's one-shot path is meaningfully faster than picomatch's, because pattern compilation in Rust is cheaper than building a regex source string and handing it to V8.
- **You match against `Buffer` or `Uint8Array` data** (e.g. results from `fs.readdir(..., { encoding: 'buffer' })`). zeromatch matches bytes directly with no string conversion.

picomatch is likely a better fit if your hot loop calls a single cached matcher one path at a time and you're already at the limit there - that path is the one V8's regex engine is best at, and the FFI hop into native code is real overhead.

## Benchmark

Pattern: `**/needle.{js,ts,tsx,jsx,mdx}` against `some/a/bigger/path/to/the/crazy/needle.ts`.

| Workload | picomatch | zeromatch |
|---|---|---|
| Compile + match once | 0.48 M ops/s | **1.08 M ops/s** (2.2×) |
| Cached single match | **10.4 M ops/s** | 7.7 M ops/s |
| Cached × 1000 paths | 22.3 K ops/s (22.3 M paths/s) | 8.1 K ops/s (8.1 M paths/s) |

Numbers from `tinybench` on a Windows x64 machine. Your mileage will vary by CPU and Node version; run `bun run bench` to measure on your hardware.

Honest read: picomatch wins the single-call cached benchmark. zeromatch wins one-shot by 2×, and the gap widens further as patterns get more complex. If your workload is dominated by repeated calls against a hot, simple pattern, picomatch is hard to displace; for everything else, zeromatch is faster.

## Usage

```js
import { Glob, GlobSet, isMatch, scan, makeRe } from 'zeromatch'

// One-shot.
isMatch('src/index.ts', '**/*.{ts,tsx}')      // true

// Compiled matcher (recommended for hot loops).
const g = new Glob('**/*.{ts,tsx}')
g.test('src/index.ts')                         // true
g.test('src/index.css')                        // false

// Batched.
g.testMany(['a.ts', 'b.css', 'c.tsx'])         // [true, false, true]
g.filter(['a.ts', 'b.css', 'c.tsx'])           // ['a.ts', 'c.tsx']

// Many patterns at once.
const set = new GlobSet(['**/*.ts', '!**/node_modules/**'])
set.test('src/app.ts')                         // true
set.matches('src/app.ts')                      // [0]

// Direct byte input (zero-copy from Buffer / Uint8Array).
g.testBuffer(Buffer.from('src/index.ts'))      // true

// picomatch-compatible scan().
scan('src/**/*.js')
// → { base: 'src', glob: '**/*.js', isGlob: true, isGlobstar: true, ... }

// Get the underlying regex source if you need it.
makeRe('*.js')                                 // '^(?:[^/]*\\.js)$'
```

## Compatibility

zeromatch aims to be a drop-in replacement for the picomatch surface most projects use - `isMatch`, the compiled matcher form, `scan`, `makeRe`. There are differences worth knowing:

- The `onMatch`/`onIgnore`/`onResult` callback options aren't implemented; they're rarely used in practice.
- `makeRe` returns the regex *source string* rather than a `RegExp` instance, since zeromatch doesn't use regexes internally and constructing one would only be useful to the caller. Wrap it in `new RegExp(...)` yourself if you need the object.
- Negation parsing follows picomatch's rules but is slightly stricter about ambiguous cases.

If you hit a real-world pattern that picomatch handles and zeromatch doesn't, please open an issue with both inputs - that's the easiest way to keep parity tight.

## Platform support

Prebuilt binaries are published for the common platforms. If you're on something unusual, the package will fall back to building from source, which requires a Rust toolchain (1.80+).

Node.js ≥ 18.17, ≥ 20.3, or ≥ 21.1.

## License

[MIT](LICENSE)
